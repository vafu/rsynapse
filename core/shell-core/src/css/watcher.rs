use std::{path::Path, sync::mpsc, thread, time::Duration};

use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind,
};

use super::{SassConfig, StylesheetError, StylesheetSource};

const STYLESHEET_DEBOUNCE: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(crate) struct StylesheetWatcher {
    _watcher: RecommendedWatcher,
    _debounce_thread: thread::JoinHandle<()>,
}

pub(crate) fn watch_stylesheet(
    source: &StylesheetSource,
    sass_config: &SassConfig,
    reload_sender: async_channel::Sender<()>,
) -> Result<StylesheetWatcher, StylesheetError> {
    let (event_sender, event_receiver) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = event_sender.send(event);
        },
        Config::default(),
    )
    .map_err(|source| StylesheetError::Watch {
        path: Path::new(".").to_path_buf(),
        source,
    })?;

    for root in source.watch_roots(sass_config)? {
        let mode = if root.is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher
            .watch(&root, mode)
            .map_err(|source| StylesheetError::Watch { path: root, source })?;
    }

    let debounce_thread = thread::spawn(move || {
        debounce_stylesheet_events(event_receiver, reload_sender);
    });

    Ok(StylesheetWatcher {
        _watcher: watcher,
        _debounce_thread: debounce_thread,
    })
}

fn debounce_stylesheet_events(
    event_receiver: mpsc::Receiver<notify::Result<Event>>,
    reload_sender: async_channel::Sender<()>,
) {
    while let Ok(event) = event_receiver.recv() {
        match event {
            Ok(event) if is_stylesheet_reload_event(&event) => {}
            Ok(_) => continue,
            Err(error) => {
                eprintln!("stylesheet watch error: {error}");
                continue;
            }
        }

        loop {
            match event_receiver.recv_timeout(STYLESHEET_DEBOUNCE) {
                Ok(Ok(event)) if is_stylesheet_reload_event(&event) => {}
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    eprintln!("stylesheet watch error: {error}");
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }

        let _ = reload_sender.try_send(());
    }
}

pub(super) fn is_stylesheet_reload_event(event: &Event) -> bool {
    is_reload_event_kind(&event.kind)
        && event
            .paths
            .iter()
            .any(|path| is_stylesheet_path(path.as_path()))
}

fn is_reload_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Name(_))
    )
}

pub(super) fn is_stylesheet_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("css")
                || extension.eq_ignore_ascii_case("scss")
                || extension.eq_ignore_ascii_case("sass")
        })
}
