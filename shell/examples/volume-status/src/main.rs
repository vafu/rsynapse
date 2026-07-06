use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use relm4::prelude::*;
use shell_core::{
    ShellApp,
    gtk::{self, prelude::*},
    source::{
        Observable,
        rx::{
            BoxedSubscriptionSend, Context, CoreObservable, IntoBoxedSubscription, Observable as _,
            ObservableType, Observer, Shared, Subscription,
        },
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct VolumeStatus {
    percent: u8,
    muted: bool,
}

impl VolumeStatus {
    fn title(&self) -> String {
        if self.muted {
            format!("{}% muted", self.percent)
        } else {
            format!("{}%", self.percent)
        }
    }

    fn detail(&self) -> &'static str {
        if self.muted {
            "Default sink is muted"
        } else {
            "Default sink is active"
        }
    }

    fn fraction(&self) -> f64 {
        f64::from(self.percent).clamp(0.0, 100.0) / 100.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum VolumeView {
    Waiting,
    Ready(VolumeStatus),
}

impl VolumeView {
    fn title(&self) -> String {
        match self {
            Self::Waiting => "Waiting for audio".to_owned(),
            Self::Ready(status) => status.title(),
        }
    }

    fn detail(&self) -> &'static str {
        match self {
            Self::Waiting => "Subscribing to PulseAudio events",
            Self::Ready(status) => status.detail(),
        }
    }

    fn fraction(&self) -> f64 {
        match self {
            Self::Waiting => 0.0,
            Self::Ready(status) => status.fraction(),
        }
    }
}

struct VolumeWindow {
    volume: VolumeView,
    last_error: Option<String>,
    _subscription: BoxedSubscriptionSend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VolumeCommand {
    Down,
    ToggleMute,
    Up,
}

#[derive(Debug)]
enum VolumeInput {
    Status(VolumeStatus),
    SourceError(String),
    Command(VolumeCommand),
}

#[relm4::component(async)]
impl SimpleAsyncComponent for VolumeWindow {
    type Init = ();
    type Input = VolumeInput;
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            set_title: Some("Volume Status"),
            set_default_size: (340, 160),

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 10,
                set_margin_top: 16,
                set_margin_bottom: 16,
                set_margin_start: 16,
                set_margin_end: 16,

                gtk::Label {
                    add_css_class: "title-1",
                    set_halign: gtk::Align::Start,
                    #[watch]
                    set_label: &model.volume.title(),
                },

                gtk::Label {
                    set_halign: gtk::Align::Start,
                    #[watch]
                    set_label: model.volume.detail(),
                },

                gtk::ProgressBar {
                    set_hexpand: true,
                    #[watch]
                    set_fraction: model.volume.fraction(),
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,

                    gtk::Button {
                        set_label: "-",
                        connect_clicked => VolumeInput::Command(VolumeCommand::Down),
                    },

                    gtk::Button {
                        set_label: "Mute",
                        connect_clicked => VolumeInput::Command(VolumeCommand::ToggleMute),
                    },

                    gtk::Button {
                        set_label: "+",
                        connect_clicked => VolumeInput::Command(VolumeCommand::Up),
                    },
                },

                gtk::Label {
                    add_css_class: "error",
                    set_wrap: true,
                    set_halign: gtk::Align::Start,
                    #[watch]
                    set_visible: model.last_error.is_some(),
                    #[watch]
                    set_label: model.last_error.as_deref().unwrap_or(""),
                }
            }
        }
    }

    async fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let subscription = volume_status()
            .subscribe_with(VolumeObserver(sender.input_sender().clone()))
            .into_boxed();

        let model = Self {
            volume: VolumeView::Waiting,
            last_error: None,
            _subscription: subscription,
        };
        let widgets = view_output!();
        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>) {
        match msg {
            VolumeInput::Status(status) => {
                self.volume = VolumeView::Ready(status);
                self.last_error = None;
            }
            VolumeInput::SourceError(error) => {
                self.last_error = Some(error);
            }
            VolumeInput::Command(command) => {
                if let Err(error) = apply_volume_command(command) {
                    self.last_error = Some(error);
                }
            }
        }
    }
}

struct VolumeObserver(relm4::Sender<VolumeInput>);

impl Observer<VolumeStatus, String> for VolumeObserver {
    fn next(&mut self, value: VolumeStatus) {
        self.0.emit(VolumeInput::Status(value));
    }

    fn error(self, error: String) {
        self.0.emit(VolumeInput::SourceError(error));
    }

    fn complete(self) {}

    fn is_closed(&self) -> bool {
        false
    }
}

fn volume_status() -> Observable<VolumeStatus> {
    Shared::<()>::lift(VolumeStatusSource).box_it()
}

struct VolumeStatusSource;

impl ObservableType for VolumeStatusSource {
    type Item<'a> = VolumeStatus;
    type Err = String;
}

impl<C> CoreObservable<C> for VolumeStatusSource
where
    C: Context,
    C::Inner: Observer<VolumeStatus, String> + Send + 'static,
{
    type Unsub = VolumeStatusSubscription;

    fn subscribe(self, context: C) -> Self::Unsub {
        let child = Arc::new(Mutex::new(None));
        let child_for_thread = child.clone();
        let mut observer = context.into_inner();

        let handle = thread::spawn(move || {
            match read_volume_status() {
                Ok(status) => observer.next(status),
                Err(error) => {
                    observer.error(error);
                    return;
                }
            }

            let mut process = match Command::new("pactl")
                .arg("subscribe")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(process) => process,
                Err(error) => {
                    observer.error(format!("start pactl subscribe failed: {error}"));
                    return;
                }
            };

            let Some(stdout) = process.stdout.take() else {
                observer.error("pactl subscribe did not provide stdout".to_owned());
                return;
            };

            *child_for_thread
                .lock()
                .expect("volume source child lock poisoned") = Some(process);

            let mut last_status = None;
            for line in BufReader::new(stdout).lines() {
                if observer.is_closed() {
                    return;
                }

                let line = match line {
                    Ok(line) => line,
                    Err(error) => {
                        observer.error(format!("read pactl subscribe output failed: {error}"));
                        return;
                    }
                };

                if !is_relevant_pactl_event(&line) {
                    continue;
                }

                match read_volume_status() {
                    Ok(status) if Some(&status) != last_status.as_ref() => {
                        last_status = Some(status.clone());
                        observer.next(status);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        observer.error(error);
                        return;
                    }
                }
            }
        });

        VolumeStatusSubscription {
            child,
            handle: Some(handle),
        }
    }
}

struct VolumeStatusSubscription {
    child: Arc<Mutex<Option<Child>>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Subscription for VolumeStatusSubscription {
    fn unsubscribe(mut self) {
        self.stop();
    }

    fn is_closed(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(thread::JoinHandle::is_finished)
    }
}

impl VolumeStatusSubscription {
    fn stop(&mut self) {
        if let Some(mut child) = self
            .child
            .lock()
            .expect("volume source child lock poisoned")
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for VolumeStatusSubscription {
    fn drop(&mut self) {
        self.stop();
    }
}

fn is_relevant_pactl_event(line: &str) -> bool {
    line.contains(" on sink #") || line.contains(" on server #")
}

fn read_volume_status() -> Result<VolumeStatus, String> {
    Ok(VolumeStatus {
        percent: read_volume_percent()?,
        muted: read_muted()?,
    })
}

fn read_volume_percent() -> Result<u8, String> {
    let output = command_stdout(
        Command::new("pactl")
            .arg("get-sink-volume")
            .arg("@DEFAULT_SINK@"),
    )?;
    parse_percent(&output).ok_or_else(|| format!("could not parse pactl volume output: {output:?}"))
}

fn read_muted() -> Result<bool, String> {
    let output = command_stdout(
        Command::new("pactl")
            .arg("get-sink-mute")
            .arg("@DEFAULT_SINK@"),
    )?;
    match output.trim() {
        "Mute: yes" => Ok(true),
        "Mute: no" => Ok(false),
        value => Err(format!("could not parse pactl mute output: {value:?}")),
    }
}

fn command_stdout(command: &mut Command) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("run {command:?} failed: {error}"))?;
    if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|error| format!("decode {command:?} stdout failed: {error}"))
    } else {
        let stderr =
            String::from_utf8(output.stderr).unwrap_or_else(|_| "<non-utf8 stderr>".to_owned());
        Err(format!("{command:?} failed: {}", stderr.trim()))
    }
}

fn parse_percent(output: &str) -> Option<u8> {
    let percent_index = output.find('%')?;
    let start = output[..percent_index]
        .rfind(|character: char| !character.is_ascii_digit())
        .map_or(0, |index| index + 1);
    output[start..percent_index].parse::<u8>().ok()
}

fn apply_volume_command(command: VolumeCommand) -> Result<(), String> {
    let mut process = Command::new("pactl");
    process.arg(match command {
        VolumeCommand::Down | VolumeCommand::Up => "set-sink-volume",
        VolumeCommand::ToggleMute => "set-sink-mute",
    });
    process.arg("@DEFAULT_SINK@");
    process.arg(match command {
        VolumeCommand::Down => "-5%",
        VolumeCommand::ToggleMute => "toggle",
        VolumeCommand::Up => "+5%",
    });
    command_stdout(&mut process).map(|_| ())
}

fn main() {
    ShellApp::new("org.rsynapse.VolumeStatusExample")
        .with_relm_threads(2)
        .run_async::<VolumeWindow>(());
}

#[cfg(test)]
mod tests {
    use super::{is_relevant_pactl_event, parse_percent};

    #[test]
    fn parse_percent_reads_first_channel_percent() {
        assert_eq!(
            parse_percent("Volume: aux0: 19661 /  30% / -31.37 dB, aux1: 19661 /  30%"),
            Some(30)
        );
    }

    #[test]
    fn relevant_events_are_sink_or_server_events() {
        assert!(is_relevant_pactl_event("Event 'change' on sink #42"));
        assert!(is_relevant_pactl_event(
            "Event 'change' on server #4294967295"
        ));
        assert!(!is_relevant_pactl_event("Event 'new' on client #10"));
    }
}
