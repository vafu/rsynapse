use std::{convert::Infallible, path::PathBuf, pin::Pin, sync::Arc};

use async_channel::Sender;
use futures_util::{Stream, StreamExt, stream as futures_stream};
use rxrust::prelude::{
    Observable as RxObservable, ObservableFactory as _, Shared, SharedBoxedObservable,
};

pub mod dbus;
mod state;
mod stream;
mod support;

pub use rxrust;
pub use rxrust::prelude as rx;
pub use state::StateSignal;
pub use stream::{Source, SourceSubscription};

pub type Observable<T, E = String> = SharedBoxedObservable<'static, T, E>;

pub fn once<T>(value: T) -> Observable<T>
where
    T: Send + 'static,
{
    Shared::<()>::of(value)
        .map_err(|error: Infallible| match error {})
        .box_it()
}

/// Defers Observable construction until subscription time.
pub fn defer<T>(create: impl Fn() -> Observable<T> + Clone + Send + Sync + 'static) -> Observable<T>
where
    T: Send + 'static,
{
    Shared::<()>::defer(create).box_it()
}

/// Builds an Observable from an async task that forwards values into a channel.
///
/// Dropping the subscription aborts the task. This is the Observable-era bridge
/// for backend APIs that need one small custom async loop.
pub fn from_task<T, F, Fut>(run: F) -> Observable<T>
where
    T: Send + 'static,
    F: Fn(Sender<Result<T, String>>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    support::from_stream_result(Source::from_task(run).stream())
}

/// Combines a dynamic list of observables into one latest-value vector.
///
/// RxRust currently exposes binary `combine_latest`; this keeps the fold in one
/// place while preserving normal Observable composition at call sites.
pub fn combine_latest<T>(observables: Vec<Observable<T>>) -> Observable<Vec<T>>
where
    T: Clone + Send + 'static,
{
    let len = observables.len();
    let mut observables = observables.into_iter().enumerate();
    let Some((first_index, first)) = observables.next() else {
        return once(Vec::new());
    };

    observables
        .fold(
            first
                .map(move |value| {
                    let mut values = vec![None; len];
                    values[first_index] = Some(value);
                    values
                })
                .box_it(),
            |combined, (index, observable)| {
                combined
                    .combine_latest(observable, move |mut values, value| {
                        values[index] = Some(value);
                        values
                    })
                    .box_it()
            },
        )
        .filter_map(|values| values.into_iter().collect::<Option<Vec<_>>>())
        .box_it()
}

/// Compatibility spelling for call sites that predate `combine_latest`.
pub fn combine_latest_vec<T>(observables: Vec<Observable<T>>) -> Observable<Vec<T>>
where
    T: Clone + Send + 'static,
{
    combine_latest(observables)
}

/// Switches to the latest inner observable produced by `map`.
///
/// This is the Observable-first equivalent of Rx `switchMap`, implemented as a
/// stream bridge so boxed shared observables compose predictably.
pub fn switch_map<T, U>(
    source: Observable<T>,
    map: impl Fn(T) -> Observable<U> + Send + Sync + 'static,
) -> Observable<U>
where
    T: Send + 'static,
    U: Send + 'static,
{
    type BoxedObservableStream<T> = Pin<Box<dyn Stream<Item = Result<T, String>> + Send + 'static>>;

    struct State<T, U, F> {
        outer: BoxedObservableStream<T>,
        inner: Option<BoxedObservableStream<U>>,
        map: Arc<F>,
        done: bool,
        outer_done: bool,
    }

    let map = Arc::new(map);
    support::from_stream_result(futures_stream::unfold(
        State {
            outer: Box::pin(source.into_stream()),
            inner: None,
            map,
            done: false,
            outer_done: false,
        },
        |mut state| async move {
            loop {
                if state.done {
                    return None;
                }

                if state.outer_done {
                    let Some(inner) = state.inner.as_mut() else {
                        return None;
                    };
                    match inner.next().await {
                        Some(Ok(value)) => return Some((Ok(value), state)),
                        Some(Err(error)) => {
                            state.done = true;
                            return Some((Err(error), state));
                        }
                        None => return None,
                    }
                }

                if let Some(inner) = state.inner.as_mut() {
                    tokio::select! {
                        outer_item = state.outer.next() => match outer_item {
                            Some(Ok(value)) => {
                                state.inner = Some(Box::pin((state.map.as_ref())(value).into_stream()));
                            }
                            Some(Err(error)) => {
                                state.done = true;
                                return Some((Err(error), state));
                            }
                            None => state.outer_done = true,
                        },
                        inner_item = inner.next() => match inner_item {
                            Some(Ok(value)) => return Some((Ok(value), state)),
                            Some(Err(error)) => {
                                state.done = true;
                                return Some((Err(error), state));
                            }
                            None => state.inner = None,
                        },
                    }
                } else {
                    match state.outer.next().await {
                        Some(Ok(value)) => {
                            state.inner = Some(Box::pin((state.map.as_ref())(value).into_stream()));
                        }
                        Some(Err(error)) => {
                            state.done = true;
                            return Some((Err(error), state));
                        }
                        None => return None,
                    }
                }
            }
        },
    ))
}

/// Maps each item in the latest list to an observable and emits latest snapshots.
///
/// When the input list changes, subscriptions for the previous list are
/// cancelled. The output order matches the current input list order.
pub fn switch_map_list<T, U>(
    items: Observable<Vec<T>>,
    map: impl Fn(T) -> Observable<U> + Send + Sync + 'static,
) -> Observable<Vec<U>>
where
    T: Send + 'static,
    U: Clone + Send + 'static,
{
    switch_map(items, move |items| {
        combine_latest_vec(items.into_iter().map(&map).collect())
    })
}

/// `switch_map_list` with duplicate vector emissions removed.
pub fn switch_map_list_distinct<T, U>(
    items: Observable<Vec<T>>,
    map: impl Fn(T) -> Observable<U> + Send + Sync + 'static,
) -> Observable<Vec<U>>
where
    T: Send + 'static,
    U: Clone + PartialEq + Send + 'static,
{
    switch_map_list(items, map)
        .distinct_until_changed()
        .box_it()
}

/// Shares a source by a stable semantic descriptor.
///
/// Use this for higher-level sources that rebuild the same
/// `combine_latest`/`switch_map` graph for many widgets or rows. The descriptor
/// is process-local and should be stable for equivalent work.
pub fn shared_by_key<T>(
    kind: &'static str,
    key: impl Into<String>,
    create: impl Fn() -> Observable<T> + Send + Sync + 'static,
) -> Observable<T>
where
    T: Clone + Send + 'static,
{
    support::shared_by_key(kind, key, create)
}

/// One source error caught by shell-core source primitives.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceError {
    pub id: u64,
    pub source: &'static str,
    pub path: PathBuf,
    pub message: String,
}

/// Process-local source error state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceErrors {
    pub total: u64,
    pub recent: Vec<SourceError>,
}

/// Emits the process-local total number of source errors caught by shell-core.
///
/// The first emission is the current total. Future emissions happen when a
/// source primitive logs a hard error.
pub fn error_count() -> Observable<u64> {
    support::error_count()
}

/// Emits process-local source error totals and recent error history.
pub fn errors() -> Observable<SourceErrors> {
    support::errors()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rxrust::prelude::{Observable as _, ObservableFactory as _, Observer};

    use super::{Shared, combine_latest_vec, switch_map, switch_map_list};

    #[test]
    fn combine_latest_vec_replaces_values_by_source_index() {
        let mut first = Shared::subject::<i32, String>();
        let mut second = Shared::subject::<i32, String>();
        let emitted = Arc::new(Mutex::new(Vec::new()));

        let _subscription =
            combine_latest_vec(vec![first.clone().box_it(), second.clone().box_it()])
                .subscribe_with(CollectValues(emitted.clone()));

        first.next(1);
        assert!(emitted.lock().unwrap().is_empty());

        second.next(10);
        second.next(20);
        first.next(2);

        assert_eq!(
            emitted.lock().unwrap().as_slice(),
            &[vec![1, 10], vec![1, 20], vec![2, 20]]
        );
    }

    #[test]
    fn switch_map_uses_latest_inner_observable() {
        runtime().block_on(async {
            let mut outer = Shared::subject::<i32, String>();
            let mut first = Shared::subject::<i32, String>();
            let mut second = Shared::subject::<i32, String>();
            let first_for_map = first.clone();
            let second_for_map = second.clone();
            let emitted = Arc::new(Mutex::new(Vec::new()));
            let captured = emitted.clone();

            let source = switch_map(outer.clone().box_it(), move |value| {
                if value == 1 {
                    first_for_map.clone().box_it()
                } else {
                    second_for_map.clone().box_it()
                }
            });
            let _subscription = source.subscribe_with(CollectScalars(captured));
            tokio::task::yield_now().await;

            outer.next(1);
            tokio::task::yield_now().await;
            first.next(10);
            tokio::task::yield_now().await;
            outer.next(2);
            tokio::task::yield_now().await;
            first.next(11);
            second.next(20);
            tokio::task::yield_now().await;

            assert_eq!(emitted.lock().unwrap().as_slice(), &[10, 20]);
        });
    }

    #[test]
    fn switch_map_list_combines_latest_values_in_input_order() {
        runtime().block_on(async {
            let mut items = Shared::subject::<Vec<u32>, String>();
            let mut first = Shared::subject::<u32, String>();
            let mut second = Shared::subject::<u32, String>();
            let first_for_map = first.clone();
            let second_for_map = second.clone();
            let emitted = Arc::new(Mutex::new(Vec::new()));

            let source = switch_map_list(items.clone().box_it(), move |item| match item {
                1 => first_for_map.clone().box_it(),
                2 => second_for_map.clone().box_it(),
                value => Shared::<()>::of(value)
                    .map_err(|error| match error {})
                    .box_it(),
            });
            let _subscription = source.subscribe_with(CollectU32Vecs(emitted.clone()));
            tokio::task::yield_now().await;

            items.next(vec![1, 2]);
            tokio::task::yield_now().await;
            first.next(10);
            tokio::task::yield_now().await;
            second.next(20);
            tokio::task::yield_now().await;
            second.next(21);
            tokio::task::yield_now().await;

            assert_eq!(
                emitted.lock().unwrap().as_slice(),
                &[vec![10, 20], vec![10, 21]]
            );
        });
    }

    struct CollectValues(Arc<Mutex<Vec<Vec<i32>>>>);

    impl Observer<Vec<i32>, String> for CollectValues {
        fn next(&mut self, value: Vec<i32>) {
            self.0.lock().unwrap().push(value);
        }

        fn error(self, error: String) {
            panic!("unexpected observable error: {error}");
        }

        fn complete(self) {}

        fn is_closed(&self) -> bool {
            false
        }
    }

    struct CollectScalars(Arc<Mutex<Vec<i32>>>);

    impl Observer<i32, String> for CollectScalars {
        fn next(&mut self, value: i32) {
            self.0.lock().unwrap().push(value);
        }

        fn error(self, error: String) {
            panic!("unexpected observable error: {error}");
        }

        fn complete(self) {}

        fn is_closed(&self) -> bool {
            false
        }
    }

    struct CollectU32Vecs(Arc<Mutex<Vec<Vec<u32>>>>);

    impl Observer<Vec<u32>, String> for CollectU32Vecs {
        fn next(&mut self, value: Vec<u32>) {
            self.0.lock().unwrap().push(value);
        }

        fn error(self, error: String) {
            panic!("unexpected observable error: {error}");
        }

        fn complete(self) {}

        fn is_closed(&self) -> bool {
            false
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }
}
