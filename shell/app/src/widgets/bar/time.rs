use std::time::Duration;

use shell_core::{
    gtk::glib,
    source::{
        Observable,
        rx::{Observable as _, ObservableFactory as _, Shared},
    },
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ClockView {
    pub(super) time: String,
    pub(super) date: String,
}

pub(super) fn clock() -> Observable<ClockView> {
    Shared::<()>::interval(Duration::from_secs(1))
        .start_with(vec![0])
        .map(|_| read_clock().unwrap_or_default())
        .map_err(|error| error.to_string())
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
