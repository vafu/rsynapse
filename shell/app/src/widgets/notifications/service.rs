use std::collections::HashMap;

use relm4::Sender;
use zbus::{object_server::SignalContext, zvariant::OwnedValue};

use super::{
    NotificationsInput,
    model::{NotificationClosedReason, NotificationRequest},
};

const NOTIFICATIONS_BUS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_OBJECT_PATH: &str = "/org/freedesktop/Notifications";

pub(super) fn start(input_sender: Sender<NotificationsInput>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = run(input_sender).await {
            eprintln!("[notifications/dbus] {error}");
        }
    })
}

async fn run(input_sender: Sender<NotificationsInput>) -> zbus::Result<()> {
    let interface = FreedesktopNotifications::new(input_sender);
    let _connection = zbus::connection::Builder::session()?
        .serve_at(NOTIFICATIONS_OBJECT_PATH, interface)?
        .name(NOTIFICATIONS_BUS_NAME)?
        .build()
        .await?;

    std::future::pending::<()>().await;
    #[allow(unreachable_code)]
    Ok(())
}

struct FreedesktopNotifications {
    input_sender: Sender<NotificationsInput>,
    next_id: u32,
}

impl FreedesktopNotifications {
    fn new(input_sender: Sender<NotificationsInput>) -> Self {
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
        vec!["body", "body-markup", "persistence"]
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
