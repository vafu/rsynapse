use std::time::{SystemTime, UNIX_EPOCH};

use data_sources as source;
use data_sources::{Observable, rx::Observable as _};
use shell_core::gtk::glib;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ClockView {
    pub(super) time: String,
    pub(super) date: String,
}

pub(super) fn clock() -> Observable<ClockView> {
    // Minute-boundary scheduling: the view only changes once per minute, so do
    // not wake every second.
    source::from_task(|sender| async move {
        loop {
            match read_clock() {
                Ok(view) => {
                    if sender.send(Ok(view)).await.is_err() {
                        return;
                    }
                }
                Err(error) => {
                    if sender.send(Err(error)).await.is_err() {
                        return;
                    }
                }
            }
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            let wait = 60u64.saturating_sub(now % 60).max(1);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        }
    })
    .distinct_until_changed()
    .box_it()
}

fn read_clock() -> Result<ClockView, String> {
    let now = glib::DateTime::now_local()
        .map_err(|error| format!("failed to read local time: {error}"))?;
    let time = now
        .format("%H:%M")
        .map_err(|error| format!("failed to format clock time: {error}"))?
        .to_string();
    let date = now
        .format("%a %b %d")
        .map_err(|error| format!("failed to format clock date: {error}"))?
        .to_string();

    Ok(ClockView { time, date })
}
