use std::sync::{Arc, Mutex};

use rxrust::{
    context::Context,
    observer::Observer,
    prelude::{
        BoxedSubscriptionSend, CoreObservable, IntoBoxedSubscription, Observable as _,
        ObservableFactory as _, ObservableType, Shared, SharedSubject,
    },
};

use super::Observable;

/// Process-local mutable state exposed as a replaying Observable.
#[derive(Clone)]
pub struct StateSignal<T> {
    state: Arc<StateSignalState<T>>,
}

struct StateSignalState<T> {
    value: Mutex<T>,
    subject: Mutex<SharedSubject<'static, T, String>>,
}

impl<T> StateSignal<T>
where
    T: Clone + PartialEq + Send + 'static,
{
    pub fn new(initial: T) -> Self {
        Self {
            state: Arc::new(StateSignalState {
                value: Mutex::new(initial),
                subject: Mutex::new(Shared::subject()),
            }),
        }
    }

    pub fn observable(&self) -> Observable<T> {
        Shared::<()>::lift(StateSignalObservable {
            state: self.state.clone(),
        })
        .box_it()
    }

    pub fn get(&self) -> T {
        self.state
            .value
            .lock()
            .expect("state signal value lock poisoned")
            .clone()
    }

    pub fn set(&self, value: T) {
        let changed = {
            let mut current = self
                .state
                .value
                .lock()
                .expect("state signal value lock poisoned");
            if *current == value {
                false
            } else {
                *current = value.clone();
                true
            }
        };

        if changed {
            self.state
                .subject
                .lock()
                .expect("state signal subject lock poisoned")
                .next(value);
        }
    }

    pub fn update(&self, update: impl FnOnce(&mut T)) {
        let next = {
            let mut current = self
                .state
                .value
                .lock()
                .expect("state signal value lock poisoned");
            let previous = current.clone();
            update(&mut current);
            if *current == previous {
                None
            } else {
                Some(current.clone())
            }
        };

        if let Some(next) = next {
            self.state
                .subject
                .lock()
                .expect("state signal subject lock poisoned")
                .next(next);
        }
    }
}

struct StateSignalObservable<T> {
    state: Arc<StateSignalState<T>>,
}

impl<T> ObservableType for StateSignalObservable<T>
where
    T: Clone + Send + 'static,
{
    type Item<'a>
        = T
    where
        Self: 'a;
    type Err = String;
}

impl<T, C> CoreObservable<C> for StateSignalObservable<T>
where
    T: Clone + Send + 'static,
    C: Context,
    C::Inner: Observer<T, String> + Send + 'static,
{
    type Unsub = BoxedSubscriptionSend;

    fn subscribe(self, context: C) -> Self::Unsub {
        let mut observer = context.into_inner();
        let current = self
            .state
            .value
            .lock()
            .expect("state signal value lock poisoned")
            .clone();
        observer.next(current);
        self.state
            .subject
            .lock()
            .expect("state signal subject lock poisoned")
            .clone()
            .subscribe_with(observer)
            .into_boxed()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rxrust::{
        observer::Observer,
        prelude::{Observable as _, Subscription},
    };

    use super::StateSignal;

    #[test]
    fn state_signal_replays_initial_and_distinct_changes() {
        let signal = StateSignal::new(false);
        let values = Arc::new(Mutex::new(Vec::new()));
        let subscription = signal.observable().subscribe_with(Capture(values.clone()));

        signal.set(true);
        signal.set(true);
        signal.update(|active| *active = false);

        subscription.unsubscribe();
        assert_eq!(*values.lock().unwrap(), vec![false, true, false]);
    }

    struct Capture(Arc<Mutex<Vec<bool>>>);

    impl Observer<bool, String> for Capture {
        fn next(&mut self, value: bool) {
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
}
