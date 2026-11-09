# Architecture

Slipcard is two small native apps, one per platform, that share a design
and a protocol, not code. Each one has a tray icon, a timer, a popup and an
AnkiConnect client.

```
   ┌────────────── Slipcard (per platform) ──────────────┐
   │                                                     │
   │  tray / menu bar ──► scheduler ──► popup window     │
   │   pause, quiet hours,   timer,      word → Show →   │
   │   next now, interval    quiet hours  answer → grade │
   │                            │              │         │
   │                            ▼              ▼         │
   │                       AnkiConnect client             │
   └────────────────────────────┬────────────────────────┘
                                │ HTTP JSON, 127.0.0.1:8765
                         ┌──────▼──────┐        ┌─────────┐
                         │ Anki +      │ ◄────► │ AnkiWeb │
                         │ AnkiConnect │  sync  └─────────┘
                         └─────────────┘
```

## Why native, and why two apps

The product is window behaviour, and the behaviour lives in platform APIs that
cross-platform toolkits hide or do not expose:

- **macOS.** An `NSPanel` with `.nonactivatingPanel`, a floating window level
  and the collection behaviour `canJoinAllSpaces` + `fullScreenAuxiliary`
  appears over a full-screen app on the current Space and does not activate
  Slipcard, so the frontmost app keeps keyboard focus. The app is an
  accessory (`LSUIElement`): a menu bar item, no Dock icon.
- **Linux, COSMIC (Wayland).** The popup is a
  [layer-shell](https://wayland.app/protocols/wlr-layer-shell-unstable-v1)
  surface on the overlay layer with keyboard interactivity `none` or
  `on_demand`: the compositor does not tile it and does not give it focus.
  GTK4 + [gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell); tray
  through StatusNotifierItem.
- **Linux, GNOME + Pop Shell (X11 or Wayland).** GNOME has no layer-shell. The
  popup is a utility/dialog-type window that Pop Shell floats (or a "Floating
  Window Exceptions" rule); tray through AppIndicator.

Which Linux path ships first is decided by the proof of concept: popup
behaviour depends on the compositor, so it is tested on the real desktop
before anything is built around it.

## The parts

- **Scheduler.** Fires every N minutes (default 15), skips quiet hours and
  pauses, and never stacks popups: the next interval starts when the current
  popup closes. Plain logic with no window-system dependency, unit tested.
- **Popup.** Shows the front of a card; Show reveals the back (translation,
  example); four grade buttons. A countdown ("Close (30)") closes it if
  nobody reacts. Closing is not an answer. Clicking the popup is the only way
  it gets focus.
- **AnkiConnect client.** Uses AnkiConnect API version 6:
  - `findCards` with a deck query such as `deck:"English" is:due` for the
    next due card;
  - `cardsInfo` for the card's fields and rendered question and answer;
  - `answerCards` with ease 1–4 (Again, Hard, Good, Easy), so the review is
    recorded and scheduled by Anki itself.
  An optional AnkiConnect API key is sent when configured. When Anki is not
  running, the tray shows it and no popup appears.
- **Tray / menu bar.** Pause for an hour, quiet hours, "next word now",
  interval, quit; shows the connection state.

## What Slipcard does not do

No own spaced-repetition algorithm, no own sync, no card storage, no card
editor. Anki is the database and the scheduler; Slipcard is one more place
to review. This keeps the apps small and keeps reviews consistent with
AnkiWeb, AnkiDroid and AnkiMobile.

## Repository layout

```
macos/           Swift package: the macOS app
linux/           GTK4 app (after the Linux proof of concept)
docs/            this documentation
scripts/         git hooks and repository checks
```
