use std::sync::OnceLock;

use shell_core::source::{Observable, StateSignal};

use crate::request::HintsAction;

pub(crate) fn hints_active() -> Observable<bool> {
    state().observable()
}

pub(crate) fn apply(action: HintsAction) {
    match action {
        HintsAction::Set(active) => set_active(active),
        HintsAction::Toggle => set_active(!active()),
    }
}

fn active() -> bool {
    state().get()
}

fn set_active(active: bool) {
    state().set(active);
}

fn state() -> &'static StateSignal<bool> {
    static STATE: OnceLock<StateSignal<bool>> = OnceLock::new();
    STATE.get_or_init(|| StateSignal::new(false))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use shell_core::source::rx::{Observable as _, Observer, Subscription};

    use super::{HintsAction, apply, hints_active};

    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<bool>>>);

    impl Observer<bool, String> for Capture {
        fn next(&mut self, value: bool) {
            self.0.lock().unwrap().push(value);
        }

        fn error(self, _err: String) {}

        fn complete(self) {}

        fn is_closed(&self) -> bool {
            false
        }
    }

    #[test]
    fn hints_observable_emits_initial_and_changes() {
        apply(HintsAction::Set(false));
        let values = Arc::new(Mutex::new(Vec::new()));
        let subscription = hints_active().subscribe_with(Capture(values.clone()));

        apply(HintsAction::Set(true));
        apply(HintsAction::Set(true));
        apply(HintsAction::Toggle);

        subscription.unsubscribe();
        assert_eq!(*values.lock().unwrap(), vec![false, true, false]);
    }
}
