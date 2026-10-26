# Popups

`Popup` is this fork's wrapper around the Wayland `xdg_popup` surface
role. Popups are the right primitive for menus, tooltips, hover
cards, and any transient surface that should:

- Position itself relative to a parent surface, with the compositor
  flipping/sliding to stay on-screen.
- Be dismissed by the compositor on outside-click (grab mode).
- Receive keyboard focus automatically when it grabs, and give it
  back on dismiss.
- Nest, so a submenu is a popup of a popup.

## Minimal example

```rust
use amane::{App, Color, Full, Keyboard, Layer, LayerWindow, Popup,
            Gravity, ConstraintAdjustment, Parent, Rectangle};

fn anchor() -> LayerWindow {
    LayerWindow::new()
        .width(Full).height(Full)
        .layer(Layer::Overlay)
        .keyboard(Keyboard::None)
        .namespace("my.anchor")
        .visible(true)
        .child(Rectangle::new().fill(Color::TRANSPARENT))
}

fn menu() -> Popup {
    Popup::new("my.menu")
        .size(200, 300)
        .anchor_rect(100, 100, 1, 1)
        .parent("my.anchor")
        .gravity(Gravity::BottomRight)
        .constraint_adjustment(
            ConstraintAdjustment::SLIDE_X
                | ConstraintAdjustment::SLIDE_Y
                | ConstraintAdjustment::FLIP_X
                | ConstraintAdjustment::FLIP_Y,
        )
        .grab()
        .visible(true)
        .on_dismiss(|| { /* close your menu state */ })
        .child(Rectangle::new().fill(Color::rgba(30, 30, 30, 220)))
}

fn main() {
    App::new().window(anchor).popup(menu).run();
}
```

## Builder

Required:
- `Popup::new(namespace: &'static str)` — identifier used by other
  popups when they set this one as their parent.
- `.size(w, h)` — logical size.
- `.anchor_rect(x, y, w, h)` — in parent-surface coordinates. The
  compositor positions the popup relative to this rectangle.

Optional:
- `.parent(namespace)` — a `LayerWindow`'s or another `Popup`'s
  namespace. When omitted, Amane auto-parents to the first live
  layer window.
- `.anchor(Anchor)` / `.gravity(Gravity)` — see xdg_positioner. The
  defaults put the popup adjacent to the anchor rect, extending
  bottom-right.
- `.constraint_adjustment(ConstraintAdjustment)` — bitflags
  (`SLIDE_X`, `SLIDE_Y`, `FLIP_X`, `FLIP_Y`, `RESIZE_X`,
  `RESIZE_Y`). The compositor applies these to keep the popup on
  screen.
- `.offset(x, y)` — nudge the computed position.
- `.grab()` — request compositor grab on open. Required for
  outside-click dismiss and keyboard focus routing. Non-grabbing
  popups (tooltips, hover cards) stay up until you hide them.
- `.visible(bool)` — see **Visibility reconciler** below.
- `.on_key(fn)` — called on key presses when the popup owns keyboard
  focus (grab mode).
- `.on_dismiss(fn)` — called when the compositor sends
  `xdg_popup.popup_done` (outside-click under grab, parent closed,
  Esc on some compositors).
- `.child(Widget)` — the rendered content.

## Visibility reconciler

Unlike `LayerWindow`, which can be left mapped at `.visible(false)`,
xdg-popups do not survive a hide and remap. Amane reconciles this:

- The view fn you register with `App::popup(view)` is re-evaluated
  each dispatch tick.
- A `(visible, is_live)` transition triggers the right side-effect:
  `(true, false)` opens the popup; `(false, true)` closes it.
- Opening re-sends the positioner, re-requests the grab (using the
  latest input serial), and re-attaches the widget tree.

Your view fn should therefore be cheap and side-effect-free with
respect to your app state: it should read state and declare the
popup, not mutate state.

### Compositor-dismissed popups stay down

A popup the *compositor* tore down (outside-click, parent closed) is
recorded as dismissed. While it is in that state the reconciler will
**not** reopen it, even if the view fn keeps declaring `.visible(true)`
for a tick or two before your state catches up. Without this guard the
reconciler would reopen the surface every tick and re-issue
`xdg_popup.grab()` with a now-stale input serial — which some
compositors treat as a protocol violation and use to kill the
connection.

The dismissed flag is cleared the moment the view fn declares the popup
`.visible(false)` (a genuine `(false, false)` tick). So the correct
pattern is unchanged: on `on_dismiss`, set your state so the next frame
declares `.visible(false)`; a later show then opens cleanly.

## Grab + input serial

`xdg_popup.grab(seat, serial)` requires a recent input serial from
the client. The fork tracks the latest serial from any pointer or
keyboard event (`wayland/pointer.rs`, `wayland/keyboard.rs`). If no
input has happened yet (e.g. you call `.grab().visible(true)` at
process startup), the grab is skipped with a debug log and the
popup opens without one.

The practical consequence: **open a grabbing popup in response to
an input event**, not before the first click or key. A right-click
or shortcut key is a natural trigger.

A given input serial is spent on at most one grab per popup
namespace: the fork records the serial it last grabbed with and skips
re-grabbing with the same one. This keeps the per-tick reconciler from
re-issuing a stale-serial grab for an already-grabbing popup. Each
genuine open (after a real hide→show) brings a fresh serial.

## Window geometry

Before the initial commit the fork calls
`xdg_surface.set_window_geometry(0, 0, w, h)` with the popup's full
size. The window geometry is the surface's "visible" rectangle,
excluding client-drawn shadow. The fork draws no shadow, so geometry
equals the full size.

This matters for niri: its `layer-rule { popups { ... } }`
`background-effect` (blur) and `geometry-corner-radius` mask are
applied to the window-geometry rect. Without an explicit geometry the
blur/corner mask can be mis-sized or absent.

## Parenting

- Layer-surface parent: the fork calls
  `zwlr_layer_surface_v1.get_popup(xdg_popup)` before the initial
  commit. The parent's niri `layer-rule { popups { ... } }` block
  styles the popup.
- Nested popup parent: `.parent("<other-popup-namespace>")` chains
  them. Grabs nest natively per the xdg-popup protocol. The fork
  records each popup's parent namespace so it can tear a nested stack
  down in the right order (see below).
- Normal window parent: `.parent_window("<window name>")` opens the
  popup over the open normal window with that name, like a menu or a
  tooltip inside an app. It wins over `.parent(...)`. The popup is
  skipped while no such window is open.

When a normal or layer window closes, its popups (and their
descendants) are closed first, since xdg-shell forbids destroying a
parent before its popups.

## Opening popups from a handler

`App::popup` registers slots up front. An app can also open one when
it needs it, like a context menu on a right click, with `open_popup`.
Its view is a closure, so it can carry where it was asked for:

```rust,ignore
fn on_right_click() {
    let at = pointer_position();

    Menu::write().open = true;

    open_popup(move || menu(at));
}

fn menu(at: Point) -> Popup {
    Popup::new("menu")
        .size(200, 120)
        .anchor_rect(at.x as i32, at.y as i32, 1, 1)
        .parent_window("main")
        .grab()
        .visible(Menu::read().open)
        .on_dismiss(|| Menu::write().open = false)
        .child(...)
}
```

- The slot lasts until its popup closes, through `.visible(false)`,
  a click outside, or its parent closing. Then it's dropped.
- Opening a popup with the namespace of an open one replaces it.
- `pointer_position()` is where the pointer last was, measured from
  the top-left corner of the window it was on, in the same units as
  `anchor_rect`.

## Keys while a menu is open

A compositor may keep the keyboard focus on the window under a
grabbing popup (niri does for normal windows). While it does, keys go
to the newest open grabbing popup's `on_key` instead, so Escape still
closes a menu. When the compositor focuses a popup itself, that popup
keeps the keys as before.

## Dismissal flow

Compositor side (grab mode) dismisses on:
- Click outside the popup stack.
- The parent surface being destroyed.
- Esc on some compositors.

The fork routes the `popup_done` event to the matching namespace's
`on_dismiss` callback, then tears the popup down. Your state should
set `.visible(false)` on the next frame so the view fn agrees — but
even without that, the backing is gone.

### Child-first cascade teardown

xdg-shell requires nested popups be destroyed in reverse creation
order — a child `xdg_popup` before its parent. But a dismiss can land
on a *parent* while a child is still live (outside-click while a
submenu is open, or the app hiding a whole branch at once). Tearing
the parent down first would be a protocol violation.

So both the compositor-driven `popup_done` and the app-driven
`(false, true)` close path route through a cascade: the fork walks the
parent links to collect the dismissed popup and all its descendants,
then destroys them **deepest-first**. This also prunes each torn-down
popup's `on_dismiss` handler and its recorded grab serial, so no state
leaks across open/close cycles.

You get this for free — closing or dismissing a menu with any depth of
open submenus is always ordering-correct.
