use super::*;

impl MainBar {
    pub(super) fn workspace_selector_action(&mut self, action: request::ToggleAction) {
        if self._workspace_selector.is_none() {
            if action == request::ToggleAction::Set(false) {
                return;
            }
            let builder = WorkspaceSelector::builder();
            relm4::main_application().add_window(&builder.root);
            self._workspace_selector = Some(builder.launch(()).detach());
        }
        if let Some(selector) = &self._workspace_selector {
            selector.emit(match action {
                request::ToggleAction::Set(true) => WorkspaceSelectorInput::Show,
                request::ToggleAction::Set(false) => WorkspaceSelectorInput::Hide,
                request::ToggleAction::Toggle => WorkspaceSelectorInput::Toggle,
            });
        }
    }

    pub(super) fn mpris_action(&mut self, action: request::ToggleAction, hold: bool) {
        use crate::widgets::{MprisInput, MprisWindow};

        if self._media_overlay.is_none() {
            if action == request::ToggleAction::Set(false) {
                return;
            }
            let builder = MprisWindow::builder();
            relm4::main_application().add_window(&builder.root);
            self._media_overlay = Some(
                builder
                    .launch(data_sources::StateSignal::new(false))
                    .detach(),
            );
        }
        if let Some(overlay) = &self._media_overlay {
            overlay.emit(match action {
                request::ToggleAction::Set(true) => MprisInput::Show { hold },
                request::ToggleAction::Set(false) => MprisInput::Hide,
                request::ToggleAction::Toggle => MprisInput::Toggle { hold },
            });
        }
    }
}
