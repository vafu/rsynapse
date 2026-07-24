use std::collections::HashMap;

use async_channel::{Receiver, Sender as AsyncSender};
use relm4::Sender as RelmSender;
use zbus::{object_server::SignalContext, zvariant::OwnedValue};

use super::{
    NotificationsInput,
    model::{NotificationClosedReason, NotificationRequest},
};

pub(super) const NOTIFICATIONS_BUS_NAME: &str = "org.freedesktop.Notifications";
pub(super) const NOTIFICATIONS_OBJECT_PATH: &str = "/org/freedesktop/Notifications";
pub(super) const RSYNAPSE_NOTIFICATIONS_INTERFACE: &str = "org.rsynapse.Notifications1";

pub(super) struct NotificationsService {
    _task: tokio::task::JoinHandle<()>,
    state: NotificationStateHandle,
}

impl NotificationsService {
    pub(super) fn set_has_items(&self, has_items: bool) {
        self.state.set_has_items(has_items);
    }
}

#[derive(Clone)]
struct NotificationStateHandle {
    updates: AsyncSender<bool>,
}

impl NotificationStateHandle {
    fn set_has_items(&self, has_items: bool) {
        let _ = self.updates.try_send(has_items);
    }
}

pub(super) fn start(input_sender: RelmSender<NotificationsInput>) -> NotificationsService {
    let (state_sender, state_receiver) = async_channel::unbounded();
    let task = tokio::spawn(async move {
        if let Err(error) = run(input_sender, state_receiver).await {
            eprintln!("[notifications/dbus] {error}");
        }
    });

    NotificationsService {
        _task: task,
        state: NotificationStateHandle {
            updates: state_sender,
        },
    }
}

async fn run(
    input_sender: RelmSender<NotificationsInput>,
    state_updates: Receiver<bool>,
) -> zbus::Result<()> {
    let interface = FreedesktopNotifications::new(input_sender);
    let _connection = zbus::connection::Builder::session()?
        .serve_at(NOTIFICATIONS_OBJECT_PATH, interface)?
        .serve_at(NOTIFICATIONS_OBJECT_PATH, NotificationState::default())?
        .name(NOTIFICATIONS_BUS_NAME)?
        .build()
        .await?;

    publish_notification_state(&_connection, state_updates).await
}

async fn publish_notification_state(
    connection: &zbus::Connection,
    updates: Receiver<bool>,
) -> zbus::Result<()> {
    let iface_ref = connection
        .object_server()
        .interface::<_, NotificationState>(NOTIFICATIONS_OBJECT_PATH)
        .await?;

    while let Ok(has_items) = updates.recv().await {
        let mut state = iface_ref.get_mut().await;
        if state.set_has_items(has_items) {
            state.has_items_changed(iface_ref.signal_context()).await?;
        }
    }

    Ok(())
}

struct FreedesktopNotifications {
    input_sender: RelmSender<NotificationsInput>,
    next_id: u32,
}

impl FreedesktopNotifications {
    fn new(input_sender: RelmSender<NotificationsInput>) -> Self {
        Self {
            input_sender,
            next_id: 1,
        }
    }

    fn next_notification_id(&mut self) -> u32 {
        let id = self.next_id.max(1);
        self.next_id = id.wrapping_add(1).max(1);
        id
    }

    fn emit(&self, input: NotificationsInput) -> zbus::fdo::Result<()> {
        self.input_sender
            .send(input)
            .map_err(|_| zbus::fdo::Error::Failed("notifications window is gone".to_owned()))
    }
}

#[derive(Default)]
struct NotificationState {
    has_items: bool,
}

impl NotificationState {
    fn set_has_items(&mut self, has_items: bool) -> bool {
        if self.has_items == has_items {
            return false;
        }
        self.has_items = has_items;
        true
    }
}

#[zbus::interface(name = "org.rsynapse.Notifications1")]
impl NotificationState {
    #[zbus(property)]
    fn has_items(&self) -> bool {
        self.has_items
    }
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl FreedesktopNotifications {
    async fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        let id = if replaces_id == 0 {
            self.next_notification_id()
        } else {
            replaces_id
        };

        let request = NotificationRequest::new(
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions,
            hints,
            expire_timeout,
        );
        self.emit(NotificationsInput::Show(request))?;
        Ok(id)
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_context)] signal_context: SignalContext<'_>,
    ) -> zbus::fdo::Result<()> {
        let reason = NotificationClosedReason::Dismissed;
        self.emit(NotificationsInput::Close { id, reason })?;
        Self::notification_closed(&signal_context, id, reason.code())
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<&'static str> {
        vec!["actions", "body", "body-markup", "persistence"]
    }

    #[zbus(out_args("name", "vendor", "version", "spec_version"))]
    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        (
            "Rsynapse Shell",
            "rsynapse",
            env!("CARGO_PKG_VERSION"),
            "1.2",
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        signal_context: &SignalContext<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        signal_context: &SignalContext<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}
