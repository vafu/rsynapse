# Tasks & Notes Surface

Read `README.md` and the parent shell architecture/source guidance before work.

## Product boundary

- This is a component inside `rsynapse-shell`, not a standalone application.
- The bar goal widget and Umbriel Mod+N open this overlay. Do not add a separate
  bar icon or restore a task popover as a competing entry point.
- It must remain a **layer-shell overlay**, configured before realization through
  the existing shell window primitives. The namespace is `rsynapse-workspace`.
- Center the surface, reserve no workspace space, and fit its initial size to
  logical monitor bounds. It must not enter normal compositor tiling/window lists.
- Acquire keyboard interaction while visible and release it on hide. Escape,
  Ctrl+W and close must share the same draft-safe navigation path.
- Retain native Adwaita/Relm4 layout and GtkSourceView Markdown source editing.
  Keep adaptive GTK sizing and dialog presentation; layer-shell owns placement.

## Data and sources

- Projd owns managed tasks, hierarchy, selection and history. Remarked owns notes.
- Resolve project notebooks from stored project CWD as `.notes/`. Membership is
  many-to-many through frontmatter `task/<immutable-instance-id>` tags; no primary
  note, task-state-from-checkbox model or Locus membership mirror.
- Compose existing shell-source Observables and typed service contracts. Keep
  service adapters/view models here, outside generic framework crates.
- Subscribe before loading snapshots and scope subscriptions to service owners.
  Never add filesystem watches, polling or debounce-based state correctness.
- Pin a draft's notebook/note identity. Read tags and text from the same revision
  and save them atomically. Stale/background responses must not replace drafts.
- One-time legacy context import is an explicit client workflow, not live file IPC.

## Verification

Use `test-ui.sh` for the private headless Wayland/D-Bus test by default; it must
not grab the live desktop's keyboard. Preserve its isolated runtime and portal
cleanup. Cover layer role, focus release, draft protection and real service flows.
Keep surface files focused and under 300 lines where practical.
