use std::time::Duration;

use relm4::prelude::*;
use shell_core::{
    gtk::{self, gdk, prelude::*},
    gtk4_layer_shell::{KeyboardMode, LayerShell},
};

use super::{MprisInput, MprisWindow, actions, sone, sone::SoneLikeState};

impl MprisWindow {
    pub(super) fn handle_input(&mut self, message: MprisInput, sender: ComponentSender<Self>) {
        match message {
            MprisInput::Source(message) => {
                Self::update(self, message);
                if self.changed(super::sources::Field::Player) {
                    self.sync_sone(&sender);
                }
            }
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
            MprisInput::ToggleLike => self.toggle_like(&sender),
            MprisInput::LikeStatus { track_id, result } => {
                if self.sone_track_id.as_deref() != Some(track_id.as_str()) {
                    return;
                }
                self.sone_like = match result {
                    Ok(true) => SoneLikeState::Liked,
                    Ok(false) => SoneLikeState::NotLiked,
                    Err(error) => {
                        eprintln!("[rsynapse-mpris] {error}");
                        SoneLikeState::Unavailable
                    }
                };
            }
            MprisInput::LikeResult {
                track_id,
                liked,
                result,
            } => {
                if self.sone_track_id.as_deref() != Some(track_id.as_str()) {
                    return;
                }
                self.sone_busy = false;
                match result {
                    Ok(()) => {
                        self.sone_like = if liked {
                            SoneLikeState::Liked
                        } else {
                            SoneLikeState::NotLiked
                        };
                    }
                    Err(error) => eprintln!("[rsynapse-mpris] {error}"),
                }
            }
        }
    }

    fn sync_sone(&mut self, sender: &ComponentSender<Self>) {
        let track_id = if sone::is_sone(&self.player) {
            sone::track_number(&self.player.track_id)
        } else {
            None
        };
        if self.sone_track_id == track_id {
            return;
        }

        self.sone_track_id = track_id.clone();
        self.sone_busy = false;
        let Some(track_id) = track_id else {
            self.sone_like = SoneLikeState::Hidden;
            return;
        };

        self.sone_like = SoneLikeState::Checking;
        let input = sender.input_sender().clone();
        relm4::spawn(async move {
            let result = sone::status(&track_id).await;
            let _ = input.send(MprisInput::LikeStatus { track_id, result });
        });
    }

    fn toggle_like(&mut self, sender: &ComponentSender<Self>) {
        if self.sone_busy {
            return;
        }
        let Some(track_id) = self.sone_track_id.clone() else {
            return;
        };
        let Some(liked) = self.sone_like.liked() else {
            return;
        };

        self.sone_busy = true;
        let next = !liked;
        let input = sender.input_sender().clone();
        relm4::spawn(async move {
            let result = sone::set_liked(&track_id, next).await;
            let _ = input.send(MprisInput::LikeResult {
                track_id,
                liked: next,
                result,
            });
        });
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
