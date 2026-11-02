# Adwaita-first guideline for rsynapse-remarked

This surface should feel like a native GNOME shell overlay, not a custom GTK
app. Prefer libadwaita widgets and style classes; only add custom CSS for the
overlay transparency and small polish that Adwaita does not provide.

## Layout patterns

- Root window: `AdwApplicationWindow`. Never use a plain `gtk::Window`.
- Coarse navigation between surfaces: `AdwCarousel`.
- Collapsible side panels inside a pane: `AdwOverlaySplitView`.
- Switching between two views in the same pane: `AdwViewStack` +
  `AdwInlineViewSwitcher`.
- Keep content readable on wide monitors: wrap text-heavy areas in `AdwClamp`.
- Global toasts/errors: wrap the root in `AdwToastOverlay`.

## Lists and rows

- Use `gtk::ListBox` with `.boxed-list` for vertical lists.
- Use `AdwActionRow` for items with a title, subtitle and optional suffix.
- Use `AdwExpanderRow` for items that reveal more content.
- Avoid hand-rolled list item widgets.

## Metadata / controls panels

- Use `AdwPreferencesGroup` + `AdwActionRow` / `AdwComboRow` /
  `AdwSwitchRow` / `AdwEntryRow` for read-only or editable metadata.
- Use `AdwStatusPage` for empty/placeholder states.

## Buttons and chips

- Use `AdwButtonContent` for buttons with icon + label.
- Use Adwaita style classes: `.pill` for rounded chips, `.card` for elevated
  surfaces, `.flat` for subtle controls.
- Only add custom CSS for pill rounding if the built-in `.pill` class is not
  sufficient.

## Editor

- Use `GtkSourceView` (sourceview5) for Markdown source editing.
- Place it inside a `gtk::ScrolledWindow`.
- Add the `.frame` class or wrap in a card for a bounded editor look.

## What to avoid

- Manual header bars (`gtk::HeaderBar`); use `AdwToolbarView` or `AdwHeaderBar`
  when a real chrome bar is needed.
- Hand-rolled cards with custom backgrounds; use `.card`.
- `gtk::Notebook` or `gtk::StackSwitcher`; use `AdwViewStack` +
  `AdwInlineViewSwitcher` instead.
- Heavy custom drawing or fixed-position widgets unless required by the overlay
  shell concept.

## Overlay-specific CSS

The window and its immediate container must be transparent so the shell
surface does not look like a boxed dialog:

```css
.remarked-window {
    background: transparent;
}
.remarked-root {
    background: transparent;
}
```

Cards and panels inside the overlay may use semi-transparent dark backgrounds
so text remains readable while the desktop shows through.

## Accessibility

- Give lists and panes meaningful accessible names.
- Use `AdwCarouselIndicatorDots` or `AdwCarouselIndicatorLines` if we expose
  the carousel pages to pointer users.
- Keep focus order left-to-right, top-to-bottom, matching the carousel flow.
