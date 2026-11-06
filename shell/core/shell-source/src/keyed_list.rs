//! Keyed dynamic collection sources with incremental diff emissions.
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    path::Path,
    sync::Arc,
};

use futures_util::{
    StreamExt,
    future::{AbortHandle, Abortable},
};
use rxrust::prelude::Observable as _;

use crate::{
    Observable, from_task,
    list_diff::{ListDiff, diff_slices},
};

/// Tracks a dynamic keyed collection while keeping one subscription per key.
///
/// Existing keys are not re-subscribed when the outer item list changes. Added
/// keys are mapped to their value source, removed keys drop their subscription,
/// and every change is emitted as a sequential [`ListDiff`].
pub fn keyed_list<T, K, U, Items, Key, Value>(
    items: Items,
    key: Key,
    value: Value,
) -> Observable<ListDiff<U>>
where
    T: Clone + PartialEq + Send + 'static,
    K: Clone + Eq + Hash + Send + Sync + 'static,
    U: Clone + PartialEq + Send + Sync + 'static,
    Items: Fn() -> Observable<Vec<T>> + Send + Sync + 'static,
    Key: Fn(&T) -> K + Send + Sync + 'static,
    Value: Fn(T) -> Observable<U> + Send + Sync + 'static,
{
    let items = Arc::new(items);
    let key = Arc::new(key);
    let value = Arc::new(value);
    from_task(move |sender| {
        let items = items.clone();
        let key = key.clone();
        let value = value.clone();
        async move {
            run_keyed_list(items, key, value, sender).await;
        }
    })
}

/// Applies a diff stream to a materialized list for consumers that need the
/// latest full value, such as a replaying shared source.
pub fn materialize_list<T, Source>(source: Source) -> Observable<Vec<T>>
where
    T: Clone + PartialEq + Send + 'static,
    Source: Fn() -> Observable<ListDiff<T>> + Send + Sync + 'static,
{
    let source = Arc::new(source);
    from_task(move |sender| {
        let source = source.clone();
        async move {
            let mut stream = (source)().into_stream();
            let mut items = Vec::new();
            while let Some(item) = stream.next().await {
                match item {
                    Ok(diff) => {
                        diff.apply(&mut items);
                        if sender.send(Ok(items.clone())).await.is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error)).await;
                        return;
                    }
                }
            }
        }
    })
}

struct KeyedEvent<K, U> {
    key: K,
    value: Result<U, String>,
}

struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn subscribe_value<K, U>(
    key: K,
    observable: Observable<U>,
    sender: async_channel::Sender<KeyedEvent<K, U>>,
) -> AbortOnDrop
where
    K: Clone + Send + 'static,
    U: Send + 'static,
{
    let (abort, registration) = AbortHandle::new_pair();
    tokio::spawn(Abortable::new(
        async move {
            let mut stream = observable.into_stream();
            while let Some(item) = stream.next().await {
                let event = match item {
                    Ok(value) => KeyedEvent {
                        key: key.clone(),
                        value: Ok(value),
                    },
                    Err(error) => KeyedEvent {
                        key: key.clone(),
                        value: Err(error),
                    },
                };
                let terminal = event.value.is_err();
                if sender.send(event).await.is_err() || terminal {
                    return;
                }
            }
        },
        registration,
    ));
    AbortOnDrop(abort)
}

async fn run_keyed_list<T, K, U, Items, Key, Value>(
    items: Arc<Items>,
    key: Arc<Key>,
    value: Arc<Value>,
    sender: async_channel::Sender<Result<ListDiff<U>, String>>,
) where
    T: Clone + PartialEq + Send + 'static,
    K: Clone + Eq + Hash + Send + Sync + 'static,
    U: Clone + PartialEq + Send + Sync + 'static,
    Items: Fn() -> Observable<Vec<T>> + Send + Sync + 'static,
    Key: Fn(&T) -> K + Send + Sync + 'static,
    Value: Fn(T) -> Observable<U> + Send + Sync + 'static,
{
    tracing::trace!("keyed list subscribe");
    // Bounded: value updates apply backpressure to slow keyed-list consumers.
    let (event_sender, event_receiver) = async_channel::bounded(256);
    let mut outer = (items)().into_stream();
    let mut subscriptions = HashMap::<K, (T, AbortOnDrop)>::new();
    let mut values = HashMap::<K, U>::new();
    let mut order = Vec::<K>::new();
    let mut previous = Vec::<U>::new();

    loop {
        tokio::select! {
            outer_item = outer.next() => {
                let Some(outer_item) = outer_item else {
                    tracing::trace!("keyed list outer ended");
                    return;
                };
                let new_items = match outer_item {
                    Ok(items) => {
                        tracing::trace!(items = items.len(), "keyed list outer");
                        items
                    }
                    Err(error) => {
                        tracing::warn!(%error, "keyed list outer error");
                        let _ = sender.send(Err(error)).await;
                        return;
                    }
                };

                let mut next_order = Vec::with_capacity(new_items.len());
                let mut seen = HashSet::with_capacity(new_items.len());
                for item in new_items {
                    let item_key = (key)(&item);
                    if !seen.insert(item_key.clone()) {
                        continue;
                    }
                    next_order.push(item_key.clone());
                    match subscriptions.entry(item_key.clone()) {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            if entry.get().0 != item {
                                values.remove(&item_key);
                                let guard = subscribe_value(
                                    item_key.clone(),
                                    (value)(item.clone()),
                                    event_sender.clone(),
                                );
                                *entry.get_mut() = (item, guard);
                            }
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            let guard = subscribe_value(
                                item_key.clone(),
                                (value)(item.clone()),
                                event_sender.clone(),
                            );
                            entry.insert((item, guard));
                        }
                    }
                }

                let keep = next_order.iter().cloned().collect::<HashSet<_>>();
                let removed = subscriptions
                    .keys()
                    .filter(|key| !keep.contains(*key))
                    .cloned()
                    .collect::<Vec<_>>();
                for key in removed {
                    subscriptions.remove(&key);
                    values.remove(&key);
                }
                order = next_order;
                if !emit(&sender, &mut previous, &order, &values).await {
                    return;
                }
            }
            event = event_receiver.recv() => {
                let Ok(event) = event else { return; };
                match event.value {
                    Ok(value) => {
                        if subscriptions.contains_key(&event.key) {
                            values.insert(event.key, value);
                            if !emit(&sender, &mut previous, &order, &values).await {
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        if subscriptions.remove(&event.key).is_some() {
                            values.remove(&event.key);
                            tracing::warn!(%error, "keyed list inner error");
                            crate::support::record_source_error(
                                "keyed-list",
                                Path::new("keyed-list"),
                                &error,
                            );
                            if !emit(&sender, &mut previous, &order, &values).await {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn emit<K, U>(
    sender: &async_channel::Sender<Result<ListDiff<U>, String>>,
    previous: &mut Vec<U>,
    order: &[K],
    values: &HashMap<K, U>,
) -> bool
where
    K: Clone + Eq + Hash + Sync,
    U: Clone + PartialEq + Sync,
{
    let next = order
        .iter()
        .filter_map(|key| values.get(key).cloned())
        .collect::<Vec<_>>();
    if next == *previous {
        return true;
    }
    let diff = diff_slices(previous, &next);
    *previous = next;
    sender.send(Ok(diff)).await.is_ok()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use futures_util::{Stream, StreamExt};
    use rxrust::prelude::Observable as _;

    use super::{keyed_list, materialize_list};
    use crate::StateSignal;

    #[tokio::test]
    async fn keeps_per_key_subscriptions_and_emits_updates() {
        let outer = StateSignal::new(vec![1u32, 2]);
        let signals = Arc::new(Mutex::new(HashMap::from([
            (1, StateSignal::new(10)),
            (2, StateSignal::new(20)),
            (3, StateSignal::new(30)),
        ])));

        let outer_source = outer.clone();
        let signals_for_value = signals.clone();
        let list = materialize_list(move || {
            let outer_source = outer_source.clone();
            let signals_for_value = signals_for_value.clone();
            keyed_list(
                move || outer_source.observable(),
                |key| *key,
                move |key| {
                    signals_for_value
                        .lock()
                        .unwrap()
                        .get(&key)
                        .unwrap()
                        .observable()
                },
            )
        });
        let mut stream = list.into_stream();

        assert_eq!(next_matching(&mut stream, &[10, 20]).await, vec![10, 20]);

        signals.lock().unwrap().get(&1).unwrap().set(11);
        assert_eq!(next_matching(&mut stream, &[11, 20]).await, vec![11, 20]);

        outer.set(vec![2]);
        assert_eq!(next_matching(&mut stream, &[20]).await, vec![20]);

        outer.set(vec![2, 3]);
        assert_eq!(next_matching(&mut stream, &[20, 30]).await, vec![20, 30]);

        signals.lock().unwrap().get(&3).unwrap().set(31);
        assert_eq!(next_matching(&mut stream, &[20, 31]).await, vec![20, 31]);

        outer.set(Vec::new());
        assert_eq!(next_matching(&mut stream, &[]).await, Vec::<u32>::new());
    }

    async fn next_matching<S>(stream: &mut S, expected: &[u32]) -> Vec<u32>
    where
        S: Stream<Item = Result<Vec<u32>, String>> + Unpin,
    {
        loop {
            let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
                .await
                .expect("timed out waiting for keyed list")
                .expect("keyed list stream ended")
                .expect("keyed list error");
            if next == expected {
                return next;
            }
        }
    }
}
