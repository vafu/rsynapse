mod card;
mod source;

#[cfg(test)]
mod test;

use card::ApprovalCard;
use relm4::prelude::*;
use shell_core::{
    gtk::{self, gdk, glib, prelude::*},
    gtk4_layer_shell::LayerShell,
    list::ComponentListBoxExt,
    window::{self, Anchors, Layer, WindowConfig},
};

use source::{PendingApproval, approval_key, pending_approvals};

pub(crate) use source::focused_output_name;

#[derive(Clone, Debug)]
pub struct ApprovalOverlayInit {
    pub title: &'static str,
    pub monitor: Option<gtk::gdk::Monitor>,
}

#[derive(Debug)]
pub enum ApprovalOverlayInput {
    Source(approval_overlay_sources::Msg),
    Show,
    Hide,
    Toggle,
}

impl From<approval_overlay_sources::Msg> for ApprovalOverlayInput {
    fn from(msg: approval_overlay_sources::Msg) -> Self {
        Self::Source(msg)
    }
}

/// Fullscreen approval overlay, mirroring the AGS agent-approval window.
///
/// Cards are stacked in a scrollable column instead of the AGS swipe
/// carousel: `#[bind_list]` only reconciles `gtk::Box` containers, and card
/// selection state is row-local (see `card.rs`), so a carousel offers no
/// dataflow advantage here. Dots and h/l card navigation are dropped with
/// it; per-card j/k/1-9/Enter and Esc behavior is preserved.
#[shell_macros::model(module = approval_overlay_sources)]
pub struct ApprovalOverlay {
    #[source(pending_approvals())]
    approvals: Vec<PendingApproval>,

    visible: bool,
    dismissed_key: Option<String>,
    last_auto_open_key: String,
}

#[shell_macros::component(module = approval_overlay_sources, model = ApprovalOverlay)]
#[relm4::component(pub)]
impl SimpleComponent for ApprovalOverlay {
    type Init = ApprovalOverlayInit;
    type Input = ApprovalOverlayInput;
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            add_css_class: "agent-approval-window",
            #[watch]
            set_visible: model.visible,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 14,
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                add_css_class: "agent-approval-body",

                gtk::Label {
                    add_css_class: "agent-approval-count",
                    set_halign: gtk::Align::Center,
                    #[watch]
                    set_visible: !model.approvals.is_empty(),
                    #[watch]
                    set_label: approval_count_label(&model.approvals).as_str(),
                },

                gtk::ScrolledWindow {
                    set_hexpand: true,
                    set_vexpand: true,
                    set_hscrollbar_policy: gtk::PolicyType::Never,
                    add_css_class: "agent-approval-content-scroll",

                    #[bind_list(approvals, row = ApprovalCard)]
                    cards -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 24,
                        set_hexpand: true,
                        set_valign: gtk::Align::Center,
                    },
                },

                gtk::Label {
                    add_css_class: "agent-approval-empty",
                    set_halign: gtk::Align::Center,
                    #[watch]
                    set_visible: model.approvals.is_empty(),
                    set_label: "No pending approvals",
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        window::apply_layer_shell_config(&root, approval_window_config());
        if let Some(monitor) = init.monitor.as_ref() {
            root.set_monitor(Some(monitor));
        }
        root.set_title(Some(init.title));

        let model = ApprovalOverlay::new(false, None, String::new());
        let widgets = view_output!();

        let key_controller = gtk::EventControllerKey::new();
        let key_sender = sender.input_sender().clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                key_sender.emit(ApprovalOverlayInput::Hide);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        root.add_controller(key_controller);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ApprovalOverlayInput::Source(msg) => {
                ApprovalOverlay::update(self, msg);
                self.auto_open();
            }
            ApprovalOverlayInput::Show => {
                self.dismissed_key = None;
                self.visible = true;
            }
            ApprovalOverlayInput::Hide => {
                self.dismissed_key = current_key(&self.approvals);
                self.visible = false;
            }
            ApprovalOverlayInput::Toggle => {
                if self.visible {
                    self.dismissed_key = current_key(&self.approvals);
                    self.visible = false;
                } else {
                    self.dismissed_key = None;
                    self.visible = true;
                }
            }
        }
    }
}

impl ApprovalOverlay {
    /// Pops the overlay for each new pending request, once per request key.
    ///
    /// Unlike AGS this is not scoped to workspace-linked sessions: any new
    /// pending approval surfaces, since a blocked agent needs an answer
    /// regardless of which workspace it runs on. A manual hide pins the
    /// current key so the same request never reopens the overlay.
    fn auto_open(&mut self) {
        let Some(key) = current_key(&self.approvals) else {
            self.dismissed_key = None;
            self.visible = false;
            return;
        };
        if Some(&key) == self.dismissed_key.as_ref() {
            return;
        }
        if key != self.last_auto_open_key {
            self.last_auto_open_key = key;
            self.visible = true;
        }
    }
}

fn current_key(approvals: &[PendingApproval]) -> Option<String> {
    approvals.first().map(approval_key)
}

fn approval_count_label(approvals: &[PendingApproval]) -> String {
    match approvals.len() {
        1 => "1 pending approval".to_owned(),
        count => format!("{count} pending approvals"),
    }
}

const fn approval_window_config() -> WindowConfig {
    WindowConfig::new(Layer::Overlay)
        .with_anchors(Anchors::ALL)
        .with_namespace("rsynapse-approvals")
        .with_keyboard_interactivity(true)
}
