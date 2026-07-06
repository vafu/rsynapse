use std::{
    any::{Any, TypeId},
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use futures_util::{Stream, StreamExt};
use rxrust::{
    context::Context,
    observer::{BoxedObserverSend, IntoBoxedObserver, Observer},
    prelude::{
        BoxedSubscriptionSend, CoreObservable, IntoBoxedSubscription, Observable as _,
        ObservableFactory as _, ObservableType, Shared, SharedSubject, Subscription,
    },
};

use super::{Observable, SourceError, SourceErrors};

const MAX_SOURCE_ERRORS: usize = 20;
const SOURCE_CACHE_PRUNE_MIN_LEN: usize = 128;

pub fn from_stream_result<T, S>(stream: S) -> Observable<T>
where
    T: Send + 'static,
    S: Stream<Item = Result<T, String>> + Send + 'static,
{
    Shared::<()>::lift(AbortableStreamResult { stream }).box_it()
}

struct AbortableStreamResult<S> {
    stream: S,
}

impl<T, S> ObservableType for AbortableStreamResult<S>
where
    S: Stream<Item = Result<T, String>>,
{
    type Item<'a>
        = T
    where
        Self: 'a;
    type Err = String;
}

impl<T, S, C> CoreObservable<C> for AbortableStreamResult<S>
where
    T: Send + 'static,
    S: Stream<Item = Result<T, String>> + Send + 'static,
    C: Context,
    C::Inner: Observer<T, String> + Send + 'static,
{
    type Unsub = AbortableStreamSubscription;

    fn subscribe(self, context: C) -> Self::Unsub {
        let mut observer = context.into_inner();
        let mut stream = Box::pin(self.stream);
        let handle = tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                if observer.is_closed() {
                    return;
                }
                match result {
                    Ok(value) => observer.next(value),
                    Err(error) => {
                        observer.error(error);
                        return;
                    }
                }
            }
            observer.complete();
        });

        AbortableStreamSubscription {
            handle: Some(handle),
        }
    }
}

struct AbortableStreamSubscription {
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Subscription for AbortableStreamSubscription {
    fn unsubscribe(mut self) {
        self.abort();
    }

    fn is_closed(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(tokio::task::JoinHandle::is_finished)
    }
}

impl AbortableStreamSubscription {
    fn abort(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        handle.abort();
    }
}

impl Drop for AbortableStreamSubscription {
    fn drop(&mut self) {
        self.abort();
    }
}

pub fn shared_by_key<T>(
    kind: &'static str,
    key: impl Into<String>,
    create: impl Fn() -> Observable<T> + Send + Sync + 'static,
) -> Observable<T>
where
    T: Clone + Send + 'static,
{
    let key = SourceKey {
        kind,
        type_id: TypeId::of::<T>(),
        descriptor: key.into(),
    };

    shared_with_key(key, create)
}

fn shared_with_key<T>(
    key: SourceKey,
    create: impl Fn() -> Observable<T> + Send + Sync + 'static,
) -> Observable<T>
where
    T: Clone + Send + 'static,
{
    let mut cache = source_cache().lock().expect("source cache lock poisoned");
    let mut remove_stale_key = false;
    if let Some(cached) = cache.get(&key) {
        if let Some(hub) = cached
            .as_any()
            .downcast_ref::<CachedWeakHub<T>>()
            .and_then(|value| value.0.upgrade())
        {
            trace_source_lifecycle("cache hit", &source_key_label(&key));
            return share_replay_latest(hub);
        }
        remove_stale_key = true;
    }
    if remove_stale_key {
        cache.remove(&key);
    } else if cache.len() >= SOURCE_CACHE_PRUNE_MIN_LEN {
        cache.retain(|_, cached| cached.is_alive());
    }

    trace_source_lifecycle("cache miss", &source_key_label(&key));
    let label = source_key_short_label(&key);
    let hub = Arc::new(ShareReplayHub::new(label, create));
    cache.insert(key, Box::new(CachedWeakHub(Arc::downgrade(&hub))));
    share_replay_latest(hub)
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SourceKey {
    kind: &'static str,
    type_id: TypeId,
    descriptor: String,
}

trait CachedSourceHub: Any + Send {
    fn is_alive(&self) -> bool;
    fn as_any(&self) -> &dyn Any;
}

struct CachedWeakHub<T>(Weak<ShareReplayHub<T>>);

impl<T> CachedSourceHub for CachedWeakHub<T>
where
    T: Clone + Send + 'static,
{
    fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn source_cache() -> &'static Mutex<HashMap<SourceKey, Box<dyn CachedSourceHub>>> {
    static SOURCE_CACHE: OnceLock<Mutex<HashMap<SourceKey, Box<dyn CachedSourceHub>>>> =
        OnceLock::new();
    SOURCE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn source_key_label(key: &SourceKey) -> String {
    format!("{} type={:?}", source_key_short_label(key), key.type_id)
}

fn source_key_short_label(key: &SourceKey) -> String {
    format!("{}:{}", key.kind, key.descriptor)
}

fn share_replay_latest<T>(hub: Arc<ShareReplayHub<T>>) -> Observable<T>
where
    T: Clone + Send + 'static,
{
    Shared::<()>::lift(ShareReplayLatest { hub }).box_it()
}

#[derive(Clone)]
struct ShareReplayLatest<T> {
    hub: Arc<ShareReplayHub<T>>,
}

struct ShareReplayHub<T> {
    label: String,
    create: Box<dyn Fn() -> Observable<T> + Send + Sync>,
    state: Arc<Mutex<ShareReplayState<T>>>,
}

impl<T> ShareReplayHub<T> {
    fn new(label: String, create: impl Fn() -> Observable<T> + Send + Sync + 'static) -> Self {
        Self {
            label,
            create: Box::new(create),
            state: Arc::new(Mutex::new(ShareReplayState::default())),
        }
    }
}

struct ShareReplayState<T> {
    latest: Option<T>,
    observers: Vec<ShareReplaySubscriber<T>>,
    next_observer_id: usize,
    version: u64,
    connecting: bool,
    connection: Option<BoxedSubscriptionSend>,
}

impl<T> Default for ShareReplayState<T> {
    fn default() -> Self {
        Self {
            latest: None,
            observers: Vec::new(),
            next_observer_id: 0,
            version: 0,
            connecting: false,
            connection: None,
        }
    }
}

struct ShareReplaySubscriber<T> {
    id: usize,
    observer: BoxedObserverSend<'static, T, String>,
}

impl<T> ShareReplayState<T> {
    fn add_observer(&mut self, observer: BoxedObserverSend<'static, T, String>) -> usize {
        let id = self.next_observer_id;
        self.next_observer_id += 1;
        self.observers.push(ShareReplaySubscriber { id, observer });
        id
    }

    fn remove_observer(&mut self, id: usize) -> bool {
        let Some(index) = self.observers.iter().position(|observer| observer.id == id) else {
            return false;
        };
        self.observers.remove(index);
        true
    }

    fn observer_count(&self) -> usize {
        self.observers.len()
    }

    fn replay_if_current(&mut self, id: usize, version: u64, latest: T) {
        if self.version != version {
            return;
        }

        if let Some(observer) = self.observers.iter_mut().find(|observer| observer.id == id) {
            observer.observer.next(latest);
        }
    }

    fn broadcast_next(&mut self, value: T)
    where
        T: Clone,
    {
        self.version = self.version.wrapping_add(1);
        self.latest = Some(value.clone());

        let last_index = self.observers.len().saturating_sub(1);
        for (index, observer) in self.observers.iter_mut().enumerate() {
            if index == last_index {
                observer.observer.next(value);
                break;
            }
            observer.observer.next(value.clone());
        }
    }

    fn broadcast_error(&mut self, error: String) {
        self.reset_connection();

        let mut observers = std::mem::take(&mut self.observers);
        let last_index = observers.len().saturating_sub(1);
        for (index, observer) in observers.drain(..).enumerate() {
            if index == last_index {
                observer.observer.error(error);
                break;
            }
            observer.observer.error(error.clone());
        }
    }

    fn broadcast_complete(&mut self) {
        self.reset_connection();

        for observer in std::mem::take(&mut self.observers) {
            observer.observer.complete();
        }
    }

    fn reset_connection(&mut self) {
        self.latest = None;
        self.connecting = false;
        self.connection = None;
    }
}

impl<T> ObservableType for ShareReplayLatest<T>
where
    T: Clone + Send + 'static,
{
    type Item<'a>
        = T
    where
        Self: 'a;
    type Err = String;
}

impl<T, C> CoreObservable<C> for ShareReplayLatest<T>
where
    T: Clone + Send + 'static,
    C: Context,
    C::Inner: Observer<T, String> + Send + 'static,
{
    type Unsub = ShareReplaySubscription<T>;

    fn subscribe(self, context: C) -> Self::Unsub {
        let observer = context.into_inner().into_boxed();
        let (observer_id, latest, version, should_connect) = {
            let mut state = self
                .hub
                .state
                .lock()
                .expect("share replay state lock poisoned");
            let observer_id = state.add_observer(observer);
            let should_connect = state.connection.is_none() && !state.connecting;
            if should_connect {
                state.connecting = true;
            }
            trace_source_lifecycle(
                &format!("subscribe subscribers={}", state.observer_count()),
                &self.hub.label,
            );
            (
                observer_id,
                state.latest.clone(),
                state.version,
                should_connect,
            )
        };

        if let Some(latest) = latest {
            self.hub
                .state
                .lock()
                .expect("share replay state lock poisoned")
                .replay_if_current(observer_id, version, latest);
        }

        let connection: Option<BoxedSubscriptionSend> = if should_connect {
            trace_source_lifecycle("connect", &self.hub.label);
            let observer = ShareReplayObserver {
                label: self.hub.label.clone(),
                state: self.hub.state.clone(),
            };
            Some((self.hub.create)().subscribe_with(observer).into_boxed())
        } else {
            None
        };

        if let Some(connection) = connection {
            let mut state = self
                .hub
                .state
                .lock()
                .expect("share replay state lock poisoned");
            state.connecting = false;
            if state.observer_count() == 0 {
                connection.unsubscribe();
            } else {
                state.connection = Some(connection);
            }
        }

        ShareReplaySubscription {
            label: self.hub.label.clone(),
            _hub: self.hub.clone(),
            state: self.hub.state.clone(),
            observer_id: Some(observer_id),
        }
    }
}

struct ShareReplayObserver<T> {
    label: String,
    state: Arc<Mutex<ShareReplayState<T>>>,
}

impl<T> Observer<T, String> for ShareReplayObserver<T>
where
    T: Clone + Send + 'static,
{
    fn next(&mut self, value: T) {
        let _span = tracing::trace_span!("source.emit", source = %self.label).entered();
        trace_source_emit(&self.label);
        self.state
            .lock()
            .expect("share replay state lock poisoned")
            .broadcast_next(value);
    }

    fn error(self, error: String) {
        if let Ok(mut state) = self.state.lock() {
            state.broadcast_error(error);
        }
    }

    fn complete(self) {
        if let Ok(mut state) = self.state.lock() {
            state.broadcast_complete();
        }
    }

    fn is_closed(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.observer_count() == 0)
            .unwrap_or(true)
    }
}

struct ShareReplaySubscription<T> {
    label: String,
    _hub: Arc<ShareReplayHub<T>>,
    state: Arc<Mutex<ShareReplayState<T>>>,
    observer_id: Option<usize>,
}

impl<T> Subscription for ShareReplaySubscription<T> {
    fn unsubscribe(mut self) {
        self.unsubscribe_inner();
    }

    fn is_closed(&self) -> bool {
        self.observer_id.is_none()
    }
}

impl<T> ShareReplaySubscription<T> {
    fn unsubscribe_inner(&mut self) {
        let Some(observer_id) = self.observer_id.take() else {
            return;
        };

        let connection = {
            let mut state = self.state.lock().expect("share replay state lock poisoned");
            state.remove_observer(observer_id);
            trace_source_lifecycle(
                &format!("unsubscribe subscribers={}", state.observer_count()),
                &self.label,
            );
            if state.observer_count() == 0 {
                state.latest = None;
                state.connection.take()
            } else {
                None
            }
        };

        if let Some(connection) = connection {
            trace_source_lifecycle("disconnect", &self.label);
            connection.unsubscribe();
        }
    }
}

impl<T> Drop for ShareReplaySubscription<T> {
    fn drop(&mut self) {
        // This is not a guard replacement: it is the refcount cleanup for this
        // custom shared source. Component-owned subscriptions are still guarded
        // by shell-macros via RxRust's `unsubscribe_when_dropped`.
        self.unsubscribe_inner();
    }
}

pub fn log_errors<T>(
    source: &'static str,
    path: PathBuf,
    observable: Observable<T>,
) -> Observable<T>
where
    T: Send + 'static,
{
    observable
        .map_err(move |error| {
            record_source_error(source, &path, &error);
            eprintln!("[shell-core/source/{source}] {}: {error}", path.display());
            error
        })
        .box_it()
}

pub fn error_count() -> Observable<u64> {
    errors()
        .map(|errors| errors.total)
        .distinct_until_changed()
        .box_it()
}

pub fn errors() -> Observable<SourceErrors> {
    Shared::<()>::lift(SourceErrorSnapshots).box_it()
}

struct SourceErrorSnapshots;

impl ObservableType for SourceErrorSnapshots {
    type Item<'a> = SourceErrors;
    type Err = String;
}

impl<C> CoreObservable<C> for SourceErrorSnapshots
where
    C: Context,
    C::Inner: Observer<SourceErrors, String> + Send + 'static,
{
    type Unsub = BoxedSubscriptionSend;

    fn subscribe(self, context: C) -> Self::Unsub {
        let state = source_error_state();
        let mut observer = context.into_inner();
        observer.next(source_error_snapshot(state));
        state
            .subject
            .lock()
            .expect("source error subject lock poisoned")
            .clone()
            .subscribe_with(observer)
            .into_boxed()
    }
}

struct SourceErrorState {
    total: AtomicU64,
    recent: Mutex<VecDeque<SourceError>>,
    subject: Mutex<SharedSubject<'static, SourceErrors, String>>,
}

fn source_error_state() -> &'static SourceErrorState {
    static SOURCE_ERROR_STATE: OnceLock<SourceErrorState> = OnceLock::new();
    SOURCE_ERROR_STATE.get_or_init(|| SourceErrorState {
        total: AtomicU64::new(0),
        recent: Mutex::new(VecDeque::new()),
        subject: Mutex::new(Shared::subject()),
    })
}

fn record_source_error(source: &'static str, path: &Path, message: &str) {
    let state = source_error_state();
    let total = state.total.fetch_add(1, Ordering::SeqCst) + 1;
    let snapshot = {
        let mut recent = state
            .recent
            .lock()
            .expect("source error history lock poisoned");
        recent.push_front(SourceError {
            id: total,
            source,
            path: path.to_path_buf(),
            message: message.to_owned(),
        });
        recent.truncate(MAX_SOURCE_ERRORS);
        SourceErrors {
            total,
            recent: recent.iter().cloned().collect(),
        }
    };
    if let Ok(mut subject) = state.subject.lock() {
        subject.next(snapshot);
    }
}

fn source_error_snapshot(state: &SourceErrorState) -> SourceErrors {
    SourceErrors {
        total: state.total.load(Ordering::SeqCst),
        recent: state
            .recent
            .lock()
            .expect("source error history lock poisoned")
            .iter()
            .cloned()
            .collect(),
    }
}

fn trace_source_lifecycle(action: &str, label: &str) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("SHELL_CORE_SOURCE_TRACE").is_some()) {
        if label.is_empty() {
            eprintln!("[shell-core/source] {action}");
        } else {
            eprintln!("[shell-core/source] {action} {label}");
        }
    }
}

fn trace_source_emit(label: &str) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("SHELL_CORE_SOURCE_EMIT_TRACE").is_some()) {
        eprintln!("[shell-core/source] emit {label}");
    }
}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context as TaskContext, Poll},
    };

    use futures_util::Stream;
    use rxrust::{
        observer::IntoBoxedObserver as _,
        prelude::{
            IntoBoxedSubscription as _, Observable as _, ObservableFactory as _, Observer, Shared,
            Subscription,
        },
    };

    use super::{
        ShareReplayHub, ShareReplayState, ShareReplaySubscription, from_stream_result,
        shared_by_key, source_cache,
    };

    #[derive(Clone, Default)]
    struct CountSubscription {
        unsubscribe_count: Arc<AtomicUsize>,
    }

    impl CountSubscription {
        fn count(&self) -> usize {
            self.unsubscribe_count.load(Ordering::SeqCst)
        }
    }

    impl Subscription for CountSubscription {
        fn unsubscribe(self) {
            self.unsubscribe_count.fetch_add(1, Ordering::SeqCst);
        }

        fn is_closed(&self) -> bool {
            false
        }
    }

    #[test]
    fn share_replay_subscription_drop_unsubscribes_subject_and_upstream() {
        let upstream_subscription = CountSubscription::default();
        let mut state = ShareReplayState {
            latest: Some(1),
            observers: Vec::new(),
            next_observer_id: 0,
            version: 0,
            connecting: false,
            connection: Some(upstream_subscription.clone().into_boxed()),
        };
        let observer_id = state.add_observer(IgnoreI32.into_boxed());
        let state = Arc::new(std::sync::Mutex::new(state));

        let subscription = ShareReplaySubscription {
            label: "test:/source".to_owned(),
            _hub: unused_i32_hub(),
            state: state.clone(),
            observer_id: Some(observer_id),
        };

        drop(subscription);

        assert_eq!(upstream_subscription.count(), 1);

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(state.observer_count(), 0);
        assert!(state.latest.is_none());
        assert!(state.connection.is_none());
    }

    #[test]
    fn share_replay_subscription_drop_keeps_upstream_while_other_subscribers_remain() {
        let upstream_subscription = CountSubscription::default();
        let mut state = ShareReplayState {
            latest: Some(1),
            observers: Vec::new(),
            next_observer_id: 0,
            version: 0,
            connecting: false,
            connection: Some(upstream_subscription.clone().into_boxed()),
        };
        let observer_id = state.add_observer(IgnoreI32.into_boxed());
        state.add_observer(IgnoreI32.into_boxed());
        let state = Arc::new(std::sync::Mutex::new(state));

        let subscription = ShareReplaySubscription {
            label: "test:/source".to_owned(),
            _hub: unused_i32_hub(),
            state: state.clone(),
            observer_id: Some(observer_id),
        };

        drop(subscription);

        assert_eq!(upstream_subscription.count(), 0);

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(state.observer_count(), 1);
        assert_eq!(state.latest, Some(1));
        assert!(state.connection.is_some());
    }

    #[test]
    fn shared_by_key_reuses_active_semantic_source() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let mut subject = Shared::subject::<u32, String>();
        let first_values = Arc::new(Mutex::new(Vec::new()));
        let second_values = Arc::new(Mutex::new(Vec::new()));

        let first = shared_by_key("test-derived", "same", {
            let create_count = create_count.clone();
            let subject = subject.clone();
            move || {
                create_count.fetch_add(1, Ordering::SeqCst);
                subject.clone().box_it()
            }
        });
        let second = shared_by_key("test-derived", "same", {
            let create_count = create_count.clone();
            let subject = subject.clone();
            move || {
                create_count.fetch_add(1, Ordering::SeqCst);
                subject.clone().box_it()
            }
        });

        let _first_subscription = first.subscribe_with(CollectU32(first_values.clone()));
        let _second_subscription = second.subscribe_with(CollectU32(second_values.clone()));

        assert_eq!(create_count.load(Ordering::SeqCst), 1);

        subject.next(7);

        assert_eq!(first_values.lock().unwrap().as_slice(), &[7]);
        assert_eq!(second_values.lock().unwrap().as_slice(), &[7]);
    }

    #[test]
    fn shared_by_key_replays_latest_to_late_subscriber_without_rebroadcast() {
        let mut subject = Shared::subject::<u32, String>();
        let first_values = Arc::new(Mutex::new(Vec::new()));
        let second_values = Arc::new(Mutex::new(Vec::new()));

        let first = shared_by_key("test-replay-latest", "same", {
            let subject = subject.clone();
            move || subject.clone().box_it()
        });
        let second = shared_by_key("test-replay-latest", "same", {
            let subject = subject.clone();
            move || subject.clone().box_it()
        });

        let _first_subscription = first.subscribe_with(CollectU32(first_values.clone()));
        subject.next(7);

        let _second_subscription = second.subscribe_with(CollectU32(second_values.clone()));

        assert_eq!(first_values.lock().unwrap().as_slice(), &[7]);
        assert_eq!(second_values.lock().unwrap().as_slice(), &[7]);

        subject.next(8);

        assert_eq!(first_values.lock().unwrap().as_slice(), &[7, 8]);
        assert_eq!(second_values.lock().unwrap().as_slice(), &[7, 8]);
    }

    #[test]
    fn shared_by_key_replaces_inactive_cache_entry_for_same_key() {
        let kind = "test-replace-inactive-cache-entry";
        let baseline = source_cache_count(kind);

        {
            let _dead = shared_by_key(kind, "same", || {
                Shared::<()>::of(1u32)
                    .map_err(|error: Infallible| match error {})
                    .box_it()
            });
        }

        assert_eq!(source_cache_count(kind), baseline + 1);

        {
            let _live = shared_by_key(kind, "same", || {
                Shared::<()>::of(2u32)
                    .map_err(|error: Infallible| match error {})
                    .box_it()
            });
            assert_eq!(source_cache_count(kind), baseline + 1);
        }
    }

    fn unused_i32_hub() -> Arc<ShareReplayHub<i32>> {
        Arc::new(ShareReplayHub::new("test:/source".to_owned(), || {
            Shared::<()>::of(0)
                .map_err(|error: Infallible| match error {})
                .box_it()
        }))
    }

    fn source_cache_count(kind: &'static str) -> usize {
        source_cache()
            .lock()
            .expect("source cache lock poisoned")
            .keys()
            .filter(|key| key.kind == kind)
            .count()
    }

    struct IgnoreI32;

    impl Observer<i32, String> for IgnoreI32 {
        fn next(&mut self, _value: i32) {}

        fn error(self, _err: String) {}

        fn complete(self) {}

        fn is_closed(&self) -> bool {
            false
        }
    }

    struct CollectU32(Arc<Mutex<Vec<u32>>>);

    impl Observer<u32, String> for CollectU32 {
        fn next(&mut self, value: u32) {
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

    struct PendingDropStream {
        drop_count: Arc<AtomicUsize>,
        poll_count: Arc<AtomicUsize>,
    }

    impl Stream for PendingDropStream {
        type Item = Result<(), String>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
            self.poll_count.fetch_add(1, Ordering::SeqCst);
            Poll::Pending
        }
    }

    impl Drop for PendingDropStream {
        fn drop(&mut self) {
            self.drop_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct IgnoreObserver;

    impl Observer<(), String> for IgnoreObserver {
        fn next(&mut self, _value: ()) {}

        fn error(self, _err: String) {}

        fn complete(self) {}

        fn is_closed(&self) -> bool {
            false
        }
    }

    #[test]
    fn from_stream_result_unsubscribe_drops_pending_stream() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("build test runtime");
        runtime.block_on(async {
            let drop_count = Arc::new(AtomicUsize::new(0));
            let poll_count = Arc::new(AtomicUsize::new(0));
            let stream = PendingDropStream {
                drop_count: drop_count.clone(),
                poll_count,
            };

            let subscription = from_stream_result(stream).subscribe_with(IgnoreObserver);
            subscription.unsubscribe();

            for _ in 0..10 {
                if drop_count.load(Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }

            assert_eq!(drop_count.load(Ordering::SeqCst), 1);
        });
    }
}
