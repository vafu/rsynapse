# Notifications

Standalone notification daemon and notification-center UI.

- This process owns the `org.freedesktop.Notifications` server implementation.
  Server-side zbus usage is expected here.
- Observe shell/session state through `data-sources`; do not build separate
  service clients in widgets.
- Keep notification policy, placement, and request handling app-local.
- The bar only observes `data_sources::notifications::has_notification_items()`;
  notification logic stays in this app.
