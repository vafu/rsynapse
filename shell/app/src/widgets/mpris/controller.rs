use std::time::Duration;

use relm4::prelude::*;
use shell_core::{
    gtk::{self, gdk, prelude::*},
    gtk4_layer_shell::{KeyboardMode, LayerShell},
};

use super::{MprisInput, MprisWindow, actions};

impl MprisWindow {
    pub(super) fn handle_input(&mut self, message: MprisInput, sender: ComponentSender<Self>) {
        match message {
            MprisInput::Source(message) => Self::update(self, message),
            MprisInput::Show { hold } => self.show(hold),
            MprisInput::Hide => self.hide(&sender),
            MprisInput::HideWindow(generation) if generation == self.generation => {
                self.visible = false;
                self.active.set(false);
                self.window.set_visible(false);
            }
            MprisInput::HideWindow(_) => {}
            MprisInput::Toggle { hold } => {
                if self.revealed {
                    self.hide(&sender);
                } else {
                    self.show(hold);
                }
            }
            MprisInput::Control(action) => {
                let player = self.player.player.clone();
                if player.is_empty() {
                    return;
                }
                let input = sender.input_sender().clone();
                relm4::spawn(async move {
                    let result = actions::perform(&player, action).await;
                    let _ = input.send(MprisInput::ControlResult(result));
                });
            }
            MprisInput::ControlResult(result) => {
                if let Err(error) = result {
                    eprintln!("[rsynapse-mpris] {error}");
                }
            }
        }
    }

    fn show(&mut self, hold: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.active.set(true);
        self.visible = true;
        self.revealed = true;
        self.hold = hold;
        if hold {
            self._hold_watcher.start();
        } else {
            self._hold_watcher.stop();
        }
        self.window.set_keyboard_mode(KeyboardMode::Exclusive);
        self.window.present();
        self.inhibit_shortcuts();
    }

    fn hide(&mut self, sender: &ComponentSender<Self>) {
        self.revealed = false;
        self.hold = false;
        self._hold_watcher.stop();
        self.window.set_keyboard_mode(KeyboardMode::None);
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let input = sender.input_sender().clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(180), move || {
            let _ = input.send(MprisInput::HideWindow(generation));
        });
    }

    fn inhibit_shortcuts(&self) {
        if let Some(surface) = self.window.surface() {
            if let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() {
                toplevel.inhibit_system_shortcuts(None::<&gdk::Event>);
            }
        }
    }
}
