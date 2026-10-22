use super::{Action, Actions};
use futures_util::future::{AbortHandle, Abortable};
use shell_core::gtk;
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

/// Cancels both the command future and its GTK delivery. A command already
/// accepted by projd may still finish there; its property stream is authoritative.
pub(in super::super) struct ScopedAction {
    worker: Option<AbortHandle>,
    delivery: AbortHandle,
    pending: Rc<Cell<bool>>,
    actions: Weak<Actions>,
}

impl ScopedAction {
    pub(super) fn detach(mut self) {
        self.worker.take();
    }
}

impl Drop for ScopedAction {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
            self.delivery.abort();
            if self.pending.replace(false) {
                if let Some(actions) = self.actions.upgrade() {
                    actions.busy.set(false);
                }
            }
        }
    }
}

impl Actions {
    /// Completion returns false when the caller's captured target is stale;
    /// stale commands must not produce a toast in the new target's editor.
    pub(in super::super) fn run_scoped(
        self: &Rc<Self>,
        action: Action,
        complete: impl FnOnce(bool) -> bool + 'static,
    ) -> Option<ScopedAction> {
        if self.busy.replace(true) {
            complete(false);
            return None;
        }
        // Never disable a popover anchor: only the editor controls become busy.
        let (sender, receiver) = async_channel::bounded(1);
        let (worker, registration) = AbortHandle::new_pair();
        let command = action.clone();
        relm4::spawn(Abortable::new(
            async move {
                let result = command.execute().await.map_err(|e| e.to_string());
                let _ = sender.send(result).await;
            },
            registration,
        ));
        let pending = Rc::new(Cell::new(true));
        let finished = pending.clone();
        let current = Rc::downgrade(self);
        let (delivery, registration) = AbortHandle::new_pair();
        gtk::glib::MainContext::default().spawn_local(Abortable::new(
            async move {
                let result = receiver.recv().await;
                finished.set(false);
                let Some(current) = current.upgrade() else {
                    return;
                };
                current.busy.set(false);
                if !complete(matches!(&result, Ok(Ok(())))) {
                    return;
                }
                let Some(overlay) = current.overlay.upgrade() else {
                    return;
                };
                let text = match result {
                    Ok(Ok(())) => action.success_text(),
                    Ok(Err(error)) => error,
                    Err(_) => return,
                };
                let toast = adw::Toast::new(&text);
                toast.set_use_markup(false);
                overlay.add_toast(toast);
            },
            registration,
        ));
        Some(ScopedAction {
            worker: Some(worker),
            delivery,
            pending,
            actions: Rc::downgrade(self),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_releases_only_its_own_busy_guard() {
        let actions = Rc::new(Actions {
            busy: Cell::new(true),
            overlay: gtk::glib::WeakRef::new(),
        });
        let (worker, _) = AbortHandle::new_pair();
        let (delivery, _) = AbortHandle::new_pair();
        let task = ScopedAction {
            worker: Some(worker.clone()),
            delivery: delivery.clone(),
            pending: Rc::new(Cell::new(true)),
            actions: Rc::downgrade(&actions),
        };
        drop(task);
        assert!(worker.is_aborted());
        assert!(delivery.is_aborted());
        assert!(!actions.busy.get());

        // A completed command's handle can outlive it. Dropping that handle
        // while a newer action runs must not unlock the newer operation.
        actions.busy.set(true);
        let (worker, _) = AbortHandle::new_pair();
        let (delivery, _) = AbortHandle::new_pair();
        drop(ScopedAction {
            worker: Some(worker),
            delivery,
            pending: Rc::new(Cell::new(false)),
            actions: Rc::downgrade(&actions),
        });
        assert!(actions.busy.get());
    }
}
