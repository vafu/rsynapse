use shell_core::source::{self, Observable, rx::Observable as _};
use workspace_dbus_api::Layout;

use crate::widgets::nerd_icon::{NerdIcon, md};

use super::workspace;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LayoutIndicatorView {
    pub(super) icon: NerdIcon,
    pub(super) tooltip: &'static str,
}

impl Default for LayoutIndicatorView {
    fn default() -> Self {
        view(Layout::Unknown)
    }
}

pub(super) fn status(output_name: Option<String>) -> Observable<LayoutIndicatorView> {
    source::switch_map(
        workspace::current_workspace(output_name),
        |workspace| match workspace {
            Some(workspace) => workspace.layout().map(view).box_it(),
            None => source::once(LayoutIndicatorView::default()),
        },
    )
    .distinct_until_changed()
    .box_it()
}

fn view(layout: Layout) -> LayoutIndicatorView {
    let (icon, tooltip) = match layout {
        Layout::Scrolling => (NerdIcon::move_handle(), "Layout: Scrolling"),
        Layout::Master => (NerdIcon::new(md::MD_DOCK_LEFT), "Layout: Master"),
        Layout::Dwindle => (NerdIcon::new(md::MD_VIEW_DASHBOARD), "Layout: Dwindle"),
        Layout::Unknown => (NerdIcon::workspace(), "Layout unavailable"),
    };
    LayoutIndicatorView { icon, tooltip }
}
