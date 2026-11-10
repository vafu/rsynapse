# Shell Plan

Current shell work is focused on keeping the app reactive and modular.

## Active Boundaries

- `apps/shared/data-sources` is the only service-data layer for widgets.
- `core/reactive` provides Observable/list/state machinery.
- `core/shell-core` provides Relm4 integration and list reconciliation.
- Notifications are a standalone app; the bar only observes notification state.
- The launcher is a standalone app with its own plugins.

## Follow-Ups

- Continue replacing any direct widget service/process access with shared
  adapters.
- Keep bar lists keyed and per-row subscribed.
- Improve native Adwaita list/selection styling rather than custom chrome.
- Expand `data-sources` only where an adapter is reusable across apps.

## Removed Direction

The previous Remarked/rnotes/task-notes effort, separate shell widget workspace,
and legacy task-notes action are no longer part of the shell plan.
