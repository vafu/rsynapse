use super::{State, model::Target};
use futures_util::{
    StreamExt,
    future::{AbortHandle, Abortable},
};
use shell_core::{
    gtk,
    source::{Observable, rx::Observable as _},
};
use std::rc::Rc;

pub(super) struct Watch {
    upstream: AbortHandle,
    delivery: AbortHandle,
}

impl Watch {
    pub(super) fn new(
        state: &Rc<State>,
        target: Target,
        source: Observable<Option<Vec<String>>>,
    ) -> Self {
        let (sender, receiver) = async_channel::unbounded();
        let (upstream, registration) = AbortHandle::new_pair();
        // Poll the shared observable on the shell's async runtime; GTK is only
        // touched by the weak, generation-checked delivery below.
        relm4::spawn(Abortable::new(
            async move {
                let mut stream = Box::pin(source.into_stream());
                while let Some(value) = stream.next().await {
                    if sender.send(value).await.is_err() {
                        break;
                    }
                }
            },
            registration,
        ));
        let weak = Rc::downgrade(state);
        let (delivery, registration) = AbortHandle::new_pair();
        gtk::glib::MainContext::default().spawn_local(Abortable::new(async move {
            while let Ok(value) = receiver.recv().await {
                let Some(state) = weak.upgrade() else { break; };
                match value {
                    Ok(labels) => state.receive(&target, labels),
                    Err(error) => {
                        state.receive(&target, None);
                        tracing::warn!(%error, project = %target.project, "Project labels unavailable");
                    }
                }
            }
        }, registration));
        Self { upstream, delivery }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.upstream.abort();
        self.delivery.abort();
    }
}
