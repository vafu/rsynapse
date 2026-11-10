//! Native input-idle notifications, independent of power-management inhibitors.
use crate::{Observable, from_task, rx::Observable as _};
use std::{io, time::Duration};
use tokio::io::unix::AsyncFd;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_callback, wl_registry, wl_seat},
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};

/// `true` after no seat input for `threshold`, `false` when input resumes.
/// Requires ext-idle-notify v2: inhibitors cannot hide user inactivity.
/// Uses the compositor's timer and socket readiness, never polling. Dropping
/// the subscription cancels the task and closes its Wayland connection.
pub fn input_idle(threshold: Duration) -> Observable<bool> {
    let timeout = u32::try_from(threshold.as_millis());
    crate::shared_by_key(
        "wayland.input-idle",
        threshold.as_millis().to_string(),
        move || {
            let observable = from_task(move |sender| async move {
                let result = match timeout {
                    Ok(timeout) => watch_idle(timeout, &sender).await,
                    Err(_) => Err(
                        "input idle threshold exceeds Wayland's u32 millisecond range".to_owned(),
                    ),
                };
                if let Err(error) = result {
                    let _ = sender.send(Err(error)).await;
                }
            })
            .distinct_until_changed()
            .box_it();
            crate::support::log_errors(
                "wayland-input-idle",
                std::path::PathBuf::from("wayland/input-idle"),
                observable,
            )
        },
    )
}

#[derive(Default)]
struct IdleState {
    manager: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    seat: Option<wl_seat::WlSeat>,
    globals_done: bool,
    pending: Vec<bool>,
}

async fn watch_idle(
    timeout: u32,
    sender: &async_channel::Sender<Result<bool, String>>,
) -> Result<(), String> {
    let connection =
        Connection::connect_to_env().map_err(|e| format!("connect Wayland idle source: {e}"))?;
    let mut queue = connection.new_event_queue::<IdleState>();
    let qh = queue.handle();
    let _registry = connection.display().get_registry(&qh, ());
    let _sync = connection.display().sync(&qh, ());
    let fd = connection
        .backend()
        .poll_fd()
        .try_clone_to_owned()
        .map_err(|e| e.to_string())?;
    let readable = AsyncFd::new(fd).map_err(|e| e.to_string())?;
    let mut state = IdleState::default();
    let mut notification = None;
    loop {
        queue
            .dispatch_pending(&mut state)
            .map_err(|e| format!("dispatch Wayland idle events: {e}"))?;
        if state.globals_done && notification.is_none() {
            let manager = state
                .manager
                .as_ref()
                .ok_or("compositor does not support ext-idle-notify v2 input-only notifications")?;
            let seat = state
                .seat
                .as_ref()
                .ok_or("no Wayland seat available for idle notifications")?;
            notification = Some(manager.get_input_idle_notification(timeout, seat, &qh, ()));
            state.pending.push(false); // Protocol notifications initially start active.
        }
        for idle in state.pending.drain(..) {
            if sender.send(Ok(idle)).await.is_err() {
                return Ok(());
            }
        }
        connection
            .flush()
            .map_err(|e| format!("flush Wayland idle requests: {e}"))?;
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let mut ready = readable.readable().await.map_err(|e| e.to_string())?;
        match guard.read() {
            Ok(_) => {}
            Err(wayland_client::backend::WaylandError::Io(error))
                if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("read Wayland idle events: {error}")),
        }
        ready.clear_ready();
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for IdleState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "ext_idle_notifier_v1" if version >= 2 && state.manager.is_none() => {
                    state.manager = Some(registry.bind(name, 2, qh, ()))
                }
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, 1, qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for IdleState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.globals_done = true;
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for IdleState {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for IdleState {
    fn event(
        _: &mut Self,
        _: &ext_idle_notifier_v1::ExtIdleNotifierV1,
        _: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for IdleState {
    fn event(
        state: &mut Self,
        _: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => state.pending.push(true),
            ext_idle_notification_v1::Event::Resumed => state.pending.push(false),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    #[ignore = "requires a live Wayland compositor with ext-idle-notify v2"]
    async fn dropping_subscription_closes_the_wayland_connection() {
        let fd_count = || std::fs::read_dir("/proc/self/fd").unwrap().count();
        let baseline = fd_count();
        let mut events = input_idle(Duration::from_secs(120)).into_stream();
        assert_eq!(events.next().await.unwrap().unwrap(), false);
        assert!(fd_count() >= baseline + 2);
        drop(events);
        // Cancellation is asynchronous. Observe completion without sleeps.
        for _ in 0..100 {
            tokio::task::yield_now().await;
            if fd_count() <= baseline {
                return;
            }
        }
        assert!(
            fd_count() <= baseline,
            "subscription retained its Wayland descriptors"
        );
    }
}
