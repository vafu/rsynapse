use std::fmt::Display;

use relm4::ComponentSender;

use crate::source::Observable;

/// A model whose source context can be driven by another Observable source.
///
/// This is used by `shell-macros` for nested source models such as a workspace
/// row whose current project may change over time.
pub trait SourceModel: Sized + Send + 'static {
    type Context: Clone + Send + 'static;
    type Msg: Send + 'static;

    fn from_default_context() -> Self
    where
        Self::Context: Default;

    fn update_source_model(&mut self, msg: Self::Msg);

    fn start_source_model<Component, E, Map>(
        source: Observable<Self::Context, E>,
        sender: ComponentSender<Component>,
        map: Map,
    ) -> Vec<rxrust::subscription::SubscriptionGuard<rxrust::prelude::BoxedSubscriptionSend>>
    where
        Component: relm4::Component + 'static,
        Component::Input: Send,
        Component::Output: Send,
        Component::CommandOutput: Send,
        E: Display + Send + Sync + 'static,
        Map: Fn(Self::Msg) -> Component::Input + Clone + Send + 'static;
}
