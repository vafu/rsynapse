//! Two-state opacity blink for working-agent icons.
//!
//! CSS keyframe animations and frame-clock tick callbacks both run at the
//! display refresh rate for as long as they are attached, so a "passive" bar
//! still spins a 60Hz loop to fade an icon. A two-state blink needs only a
//! single timer tick per state change, which cuts the idle cost by an order of
//! magnitude while still reading as "this agent is working".
//!
//! The timer exists only while the blink is active, honours
//! `gtk-enable-animations`, and stops when the widget leaves the screen.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use shell_core::gtk::{
    self,
    glib::{self, ControlFlow, SourceId},
    prelude::*,
};

/// Time each blink state is held.
const BLINK_INTERVAL: Duration = Duration::from_millis(500);
/// Opacity of the dimmed state.
const DIM_OPACITY: f64 = 0.4;

#[derive(Clone, Debug, Default)]
pub(super) struct AgentBlink {
    inner: Rc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    active: Cell<bool>,
    dimmed: Cell<bool>,
    source: RefCell<Option<SourceId>>,
}

impl PartialEq for AgentBlink {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl AgentBlink {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

/// Starts or stops an [`AgentBlink`] on a label from a `#[watch]` binding.
pub(super) trait AgentBlinkExt {
    fn set_agent_blink(&self, active: bool, blink: AgentBlink);
}

impl AgentBlinkExt for gtk::Label {
    fn set_agent_blink(&self, active: bool, blink: AgentBlink) {
        // Without animations, or once the widget leaves the screen, hold the
        // icon at full opacity and never schedule a timer.
        if !active || !self.is_mapped() || !animations_enabled() {
            stop(self, &blink);
            return;
        }
        if blink.inner.active.replace(true) {
            return;
        }

        let label = self.clone();
        let inner = blink.inner.clone();
        let source = glib::timeout_add_local(BLINK_INTERVAL, move || {
            if !inner.active.get() || !label.is_mapped() {
                label.set_opacity(1.0);
                inner.active.set(false);
                inner.dimmed.set(false);
                inner.source.borrow_mut().take();
                return ControlFlow::Break;
            }
            let dimmed = !inner.dimmed.get();
            inner.dimmed.set(dimmed);
            label.set_opacity(if dimmed { DIM_OPACITY } else { 1.0 });
            ControlFlow::Continue
        });
        *blink.inner.source.borrow_mut() = Some(source);
    }
}

fn stop(label: &gtk::Label, blink: &AgentBlink) {
    if blink.inner.active.replace(false) {
        if let Some(source) = blink.inner.source.borrow_mut().take() {
            source.remove();
        }
    }
    blink.inner.dimmed.set(false);
    label.set_opacity(1.0);
}

fn animations_enabled() -> bool {
    gtk::Settings::default().is_none_or(|settings| settings.is_gtk_enable_animations())
}
