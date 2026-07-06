use std::{future::Future, pin::Pin, sync::Arc};

use futures_util::{Stream, StreamExt, future, stream};

type SourceStream<T> = Pin<Box<dyn Stream<Item = Result<T, String>> + Send + 'static>>;

/// Cloneable async source used by newer shell ViewModel bindings.
///
/// `Source<T>` is intentionally small: each subscription creates an async
/// stream, while operators return another source factory. The existing
/// `Observable<T>` alias remains available during migration.
pub struct Source<T> {
    create: Arc<dyn Fn() -> SourceStream<T> + Send + Sync>,
}

impl<T> Clone for Source<T> {
    fn clone(&self) -> Self {
        Self {
            create: self.create.clone(),
        }
    }
}

impl<T> Source<T>
where
    T: Send + 'static,
{
    /// Combines a dynamic list of sources into a latest-value vector.
    pub fn combine_latest_vec(sources: Vec<Source<T>>) -> Source<Vec<T>>
    where
        T: Clone + Sync,
    {
        let len = sources.len();
        let mut sources = sources.into_iter().enumerate();
        let Some((first_index, first)) = sources.next() else {
            return Source::once(Vec::new());
        };

        sources
            .fold(
                first.map(move |value| {
                    let mut values = vec![None; len];
                    values[first_index] = Some(value);
                    values
                }),
                |combined, (index, source)| {
                    combined.combine_latest(source, move |mut values, value| {
                        values[index] = Some(value);
                        values
                    })
                },
            )
            .filter_map(|values| values.into_iter().collect::<Option<Vec<_>>>())
    }

    /// Builds a source from a stream factory.
    pub fn new(create: impl Fn() -> SourceStream<T> + Send + Sync + 'static) -> Self {
        Self {
            create: Arc::new(create),
        }
    }

    /// Emits one value and then completes.
    pub fn once(value: T) -> Self
    where
        T: Clone + Sync,
    {
        Self::new(move || {
            let value = value.clone();
            Box::pin(stream::once(async move { Ok(value) }))
        })
    }

    /// Builds a source from an async task that forwards values into a channel.
    ///
    /// Dropping the resulting stream aborts the task, which is what component
    /// subscriptions need when GTK rows or windows are destroyed.
    pub fn from_task<F, Fut>(run: F) -> Self
    where
        F: Fn(async_channel::Sender<Result<T, String>>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self::new(move || {
            let (sender, receiver) = async_channel::unbounded();
            let handle = tokio::spawn(run(sender));
            let guard = TaskGuard {
                handle: Some(handle),
            };

            Box::pin(stream::unfold(
                (receiver, guard),
                |(receiver, guard)| async move {
                    receiver
                        .recv()
                        .await
                        .ok()
                        .map(|item| (item, (receiver, guard)))
                },
            ))
        })
    }

    /// Creates a fresh stream from this source.
    pub fn stream(&self) -> SourceStream<T> {
        (self.create)()
    }

    /// Subscribes callbacks to this source.
    pub fn subscribe(
        &self,
        mut on_next: impl FnMut(T) + Send + 'static,
        mut on_error: impl FnMut(String) + Send + 'static,
    ) -> SourceSubscription {
        let mut stream = self.stream();
        let handle = tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(value) => on_next(value),
                    Err(error) => {
                        on_error(error);
                        return;
                    }
                }
            }
        });
        SourceSubscription {
            handle: Some(handle),
        }
    }

    pub fn map<U>(self, map: impl Fn(T) -> U + Send + Sync + 'static) -> Source<U>
    where
        U: Send + 'static,
    {
        let map = Arc::new(map);
        Source::new(move || {
            let map = map.clone();
            Box::pin(self.stream().map(move |item| item.map(|value| map(value))))
        })
    }

    pub fn filter_map<U>(self, map: impl Fn(T) -> Option<U> + Send + Sync + 'static) -> Source<U>
    where
        U: Send + 'static,
    {
        let map = Arc::new(map);
        Source::new(move || {
            let map = map.clone();
            Box::pin(self.stream().filter_map(move |item| {
                let item = match item {
                    Ok(value) => map(value).map(Ok),
                    Err(error) => Some(Err(error)),
                };
                future::ready(item)
            }))
        })
    }

    pub fn distinct_until_changed(self) -> Source<T>
    where
        T: Clone + PartialEq + Sync,
    {
        Source::new(move || {
            let mut latest = None::<T>;
            Box::pin(self.stream().filter_map(move |item| {
                let item = match item {
                    Ok(value) if latest.as_ref() == Some(&value) => None,
                    Ok(value) => {
                        latest = Some(value.clone());
                        Some(Ok(value))
                    }
                    Err(error) => Some(Err(error)),
                };
                future::ready(item)
            }))
        })
    }

    pub fn combine_latest<U, V>(
        self,
        other: Source<U>,
        map: impl Fn(T, U) -> V + Send + Sync + 'static,
    ) -> Source<V>
    where
        T: Clone + Sync,
        U: Clone + Send + Sync + 'static,
        V: Send + 'static,
    {
        enum Item<T, U> {
            Left(Result<T, String>),
            Right(Result<U, String>),
        }

        struct State<T, U, V> {
            stream: Pin<Box<dyn Stream<Item = Item<T, U>> + Send + 'static>>,
            left: Option<T>,
            right: Option<U>,
            map: Arc<dyn Fn(T, U) -> V + Send + Sync>,
        }

        let map: Arc<dyn Fn(T, U) -> V + Send + Sync> = Arc::new(map);
        Source::new(move || {
            let left = self.stream().map(Item::Left);
            let right = other.stream().map(Item::Right);
            let state = State {
                stream: Box::pin(stream::select(left, right)),
                left: None,
                right: None,
                map: map.clone(),
            };
            Box::pin(stream::unfold(state, |mut state| async move {
                loop {
                    match state.stream.next().await? {
                        Item::Left(Ok(value)) => state.left = Some(value),
                        Item::Right(Ok(value)) => state.right = Some(value),
                        Item::Left(Err(error)) | Item::Right(Err(error)) => {
                            return Some((Err(error), state));
                        }
                    }

                    let (Some(left), Some(right)) = (&state.left, &state.right) else {
                        continue;
                    };
                    let value = (state.map)(left.clone(), right.clone());
                    return Some((Ok(value), state));
                }
            }))
        })
    }

    pub fn switch_map<U>(self, map: impl Fn(T) -> Source<U> + Send + Sync + 'static) -> Source<U>
    where
        U: Send + 'static,
    {
        let map = Arc::new(map);
        Source::from_task(move |sender| {
            let map = map.clone();
            let mut outer = self.stream();
            async move {
                let mut inner = None::<SourceStream<U>>;
                let mut outer_done = false;

                loop {
                    if outer_done {
                        let Some(inner_stream) = inner.as_mut() else {
                            return;
                        };
                        match inner_stream.next().await {
                            Some(item) => {
                                if sender.send(item).await.is_err() {
                                    return;
                                }
                            }
                            None => return,
                        }
                        continue;
                    }

                    if let Some(inner_stream) = inner.as_mut() {
                        tokio::select! {
                            item = outer.next() => match item {
                                Some(Ok(value)) => inner = Some(map(value).stream()),
                                Some(Err(error)) => {
                                    let _ = sender.send(Err(error)).await;
                                    return;
                                }
                                None => outer_done = true,
                            },
                            item = inner_stream.next() => match item {
                                Some(item) => {
                                    if sender.send(item).await.is_err() {
                                        return;
                                    }
                                }
                                None => inner = None,
                            },
                        }
                    } else {
                        match outer.next().await {
                            Some(Ok(value)) => inner = Some(map(value).stream()),
                            Some(Err(error)) => {
                                let _ = sender.send(Err(error)).await;
                                return;
                            }
                            None => return,
                        }
                    }
                }
            }
        })
    }
}

/// Running source subscription. Dropping it aborts upstream work.
pub struct SourceSubscription {
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl SourceSubscription {
    pub fn unsubscribe(mut self) {
        self.abort();
    }

    pub fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(tokio::task::JoinHandle::is_finished)
    }

    fn abort(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for SourceSubscription {
    fn drop(&mut self) {
        self.abort();
    }
}

struct TaskGuard {
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::Source;

    #[test]
    fn map_distinct_and_combine_latest_emit_expected_values() {
        runtime().block_on(async {
            let values = Arc::new(Mutex::new(Vec::new()));
            let captured = values.clone();
            let source = Source::once(2)
                .map(|value| value * 2)
                .combine_latest(Source::once(10), |left, right| left + right)
                .distinct_until_changed();

            let _subscription = source.subscribe(
                move |value| captured.lock().unwrap().push(value),
                |error| panic!("unexpected source error: {error}"),
            );
            tokio::task::yield_now().await;

            assert_eq!(values.lock().unwrap().as_slice(), &[14]);
        });
    }

    #[test]
    fn switch_map_uses_latest_inner_source() {
        runtime().block_on(async {
            let values = Arc::new(Mutex::new(Vec::new()));
            let captured = values.clone();
            let source = Source::once(3).switch_map(|value| Source::once(value + 1));

            let _subscription = source.subscribe(
                move |value| captured.lock().unwrap().push(value),
                |error| panic!("unexpected source error: {error}"),
            );
            tokio::task::yield_now().await;

            assert_eq!(values.lock().unwrap().as_slice(), &[4]);
        });
    }

    #[test]
    fn combine_latest_vec_emits_after_all_sources_emit() {
        runtime().block_on(async {
            let values = Arc::new(Mutex::new(Vec::new()));
            let captured = values.clone();
            let source = Source::combine_latest_vec(vec![Source::once(1), Source::once(2)]);

            let _subscription = source.subscribe(
                move |value| captured.lock().unwrap().push(value),
                |error| panic!("unexpected source error: {error}"),
            );
            tokio::task::yield_now().await;

            assert_eq!(values.lock().unwrap().as_slice(), &[vec![1, 2]]);
        });
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }
}
