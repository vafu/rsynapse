//! Reactive machinery shared by shell UI crates.
//!
//! This crate contains the Observable/runtime layer and collection primitives,
//! but no service adapters. External data sources live in `data-sources`.
use std::{convert::Infallible, path::PathBuf, pin::Pin, sync::Arc};

use async_channel::Sender;
use futures_util::{Stream, StreamExt, stream as futures_stream};
use rxrust::prelude::{
    Observable as RxObservable, ObservableFactory as _, Shared, SharedBoxedObservable,
};

mod keyed_list;
mod list_diff;
mod state;
mod stream;
pub mod support;

pub use keyed_list::{keyed_list, materialize_list};
pub use list_diff::{ListChange, ListDiff, diff, diff_by_key, diff_slices, diff_slices_by_key};
pub use rxrust;
pub use rxrust::prelude as rx;
pub use state::StateSignal;
pub use stream::{Source, SourceSubscription};
pub use support::{from_stream_result, log_errors, record_source_error};

pub type Observable<T, E = String> = SharedBoxedObservable<'static, T, E>;

pub fn once<T>(value: T) -> Observable<T>
where
    T: Send + 'static,
{
    Shared::<()>::of(value)
        .map_err(|error: Infallible| match error {})
        .box_it()
}

pub fn defer<T>(create: impl Fn() -> Observable<T> + Clone + Send + Sync + 'static) -> Observable<T>
where
    T: Send + 'static,
{
    Shared::<()>::defer(create).box_it()
}

pub fn from_task<T, F, Fut>(run: F) -> Observable<T>
where
    T: Send + 'static,
    F: Fn(Sender<Result<T, String>>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    support::from_stream_result(Source::from_task(run).stream())
}

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

pub fn combine_latest_vec<T>(observables: Vec<Observable<T>>) -> Observable<Vec<T>>
where
    T: Clone + Send + 'static,
{
    combine_latest(observables)
}

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
                            Some(Ok(value)) => state.inner = Some(Box::pin((state.map.as_ref())(value).into_stream())),
                            Some(Err(error)) => { state.done = true; return Some((Err(error), state)); }
                            None => state.outer_done = true,
                        },
                        inner_item = inner.next() => match inner_item {
                            Some(Ok(value)) => return Some((Ok(value), state)),
                            Some(Err(error)) => { state.done = true; return Some((Err(error), state)); }
                            None => state.inner = None,
                        },
                    }
                } else {
                    match state.outer.next().await {
                        Some(Ok(value)) => {
                            state.inner = Some(Box::pin((state.map.as_ref())(value).into_stream()))
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceError {
    pub id: u64,
    pub source: &'static str,
    pub path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceErrors {
    pub total: u64,
    pub recent: Vec<SourceError>,
}

pub fn error_count() -> Observable<u64> {
    support::error_count()
}
pub fn errors() -> Observable<SourceErrors> {
    support::errors()
}
