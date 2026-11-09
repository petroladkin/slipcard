# Development

How to build, run and test Slipcard on each platform.

## Anki and AnkiConnect (all platforms)

Every part that talks to Anki needs a running Anki with the AnkiConnect
add-on:

1. Install [Anki](https://apps.ankiweb.net/) (desktop, 23.10 or newer).
2. Tools → Add-ons → Get Add-ons…, code `2055492159` (AnkiConnect), restart
   Anki.
3. Check it answers:

   ```sh
   curl -s localhost:8765 -X POST -d '{"action":"version","version":6}'
   # {"result": 6, "error": null}
   ```

**Use a separate Anki profile for development** (File → Switch Profile →
Add), named `Slipcard Test`. Grading cards from a test build changes their
schedule, and you do not want that in your real collection. Switch back to
your own profile when you are done.

### The test deck

`scripts/test-anki/test_deck.py` builds the test deck in a known state and
reports it (Python 3.9+, standard library only):

```sh
export SLIPCARD_ANKI_URL=http://127.0.0.1:8765   # the default
export SLIPCARD_ANKI_KEY=...                     # if AnkiConnect has an apiKey
scripts/test-anki/test_deck.py reset    # delete and rebuild the deck "Slipcard Test"
scripts/test-anki/test_deck.py status   # {"review_due": 8, "review_later": 4, "learning": 2, "new": 10}
```

It works only when the active profile is `Slipcard Test`: every command
asks AnkiConnect for the active profile first and stops before anything else
if it is another one. Integration tests follow the same rule. The tool's own
tests need no Anki: `python3 -m unittest discover -s scripts/test-anki`.

### A headless test Anki

Instead of switching profiles in your desktop Anki, you can run a test Anki
with no display on any Ubuntu 24.04 host (a VM or a container):

```sh
sudo ANKICONNECT_KEY=<a key of your choice> ANKICONNECT_BIND=0.0.0.0 \
  scripts/test-anki/setup-headless-anki.sh
```

It installs Anki from PyPI (`aqt`, pinned in the script) into a venv of a
user `anki`, creates the profile `Slipcard Test` without the first-run
dialogs, installs AnkiConnect with the API key, and runs Anki on Qt's
offscreen platform as the systemd service `slipcard-test-anki`, which comes
back after a reboot. `ANKICONNECT_BIND=0.0.0.0` makes it reachable from your
machine (`SLIPCARD_ANKI_URL=http://<host>:8765`); requests without the key
are refused. Keep a headless Anki like this for test profiles only.

### The AnkiConnect client

Both apps talk to AnkiConnect (API version 6) at the address and with the
key from the [settings](#settings); the request timeout is 3 s. The card a
popup shows comes from `findCards` with the settings' `cardQuery`, read with
`cardsInfo` (100 cards per request): learning cards first, then the most
overdue review cards, then new cards; suspended and buried cards never.
Every failure is its own state, logged and shown by the tray: Anki not
running or not reachable, no answer in time, a refused API key, an error
from AnkiConnect, or an answer that is not AnkiConnect's.

To see what a popup would show, without any window:

```sh
swift run Slipcard --next-card          # macOS (from macos/)
target/debug/slipcard --next-card       # Linux (from linux/)
# exit status: 0 a card is due, 3 none is due, 1 Anki failed
```

Unit tests run against a stub AnkiConnect replaying the replies recorded in
`testdata/ankiconnect/`, which both platforms share. Integration tests run
against a test Anki, on demand only; like `test_deck.py`, they stop before
anything else unless the active profile is `Slipcard Test`:

```sh
scripts/test-anki/test_deck.py reset
# macOS (from macos/)
SLIPCARD_ANKI_INTEGRATION=1 SLIPCARD_ANKI_URL=... SLIPCARD_ANKI_KEY=... \
  swift test --filter AnkiConnectIntegrationTests
# Linux (from linux/)
SLIPCARD_ANKI_URL=... SLIPCARD_ANKI_KEY=... cargo test -- --ignored --test-threads=1
scripts/test-anki/test_deck.py reset    # the tests grade cards
```

On macOS the binary that `swift build` makes reaches an Anki on another
host of the LAN over plain `http` with no extra permission (checked on
macOS 27). What the `Slipcard.app` bundle needs for the same (App Transport
Security, the Local Network permission) is checked when the bundle is made.

## Settings

Both apps read one JSON file at start:

- macOS: `~/Library/Application Support/Slipcard/settings.json`
- Linux: `$XDG_CONFIG_HOME/slipcard/settings.json`, or
  `~/.config/slipcard/settings.json` when `XDG_CONFIG_HOME` is not set
- `SLIPCARD_SETTINGS=<path>` overrides both, handy for development.

Every key is optional; the file can be missing. All keys at their defaults:
[`settings.example.json`](settings.example.json), which both test suites
read, so the platforms cannot drift apart.

| Key | Default | Valid values |
|---|---|---|
| `ankiConnectUrl` | `"http://127.0.0.1:8765"` | an `http://` or `https://` address |
| `ankiConnectKey` | `null` | AnkiConnect's `apiKey`, or `null` / `""` for none; sent in the request body, as AnkiConnect expects, and also as `Authorization: Bearer <key>` for a proxy in front of an AnkiConnect-compatible server that has no key of its own |
| `cardQuery` | `"is:due -is:suspended"` | an Anki search, e.g. `deck:"English" is:due -is:suspended` |
| `intervalMinutes` | `15` | a whole number, 1 to 1440 |
| `countdownSeconds` | `30` | a whole number, 5 to 600 |
| `quietHoursEnabled` | `false` | `true` or `false` |
| `quietHoursStart` | `"22:00"` | `"HH:MM"`, 24-hour clock |
| `quietHoursEnd` | `"08:00"` | `"HH:MM"`; a range may cross midnight |

A value of the wrong type or out of range is ignored with a line in the
log, and its default is used. A file that is not a JSON object is reported
(log, and the menu on macOS), the defaults are used, and Slipcard never
writes over it, so a typo in a hand edit is not lost. When the app changes
a setting it rewrites the file atomically and keeps every key it does not
know. On Linux, `slipcard --check` prints the path and what was read.

## macOS

Needs macOS 14 or newer and Xcode 16 or newer (Swift 6). The app is a Swift
package in `macos/`; Xcode is optional.

```sh
cd macos
swift build          # debug build
swift run            # starts the menu bar app
swift test           # unit tests, no display needed
```

`swift run` starts the app as a menu bar item (a book icon, no Dock icon);
"Quit Slipcard" in its menu stops it. It reads the settings, then shows a
popup every `intervalMinutes` with the next due card from AnkiConnect; with
no due card, or when Anki does not answer, no popup comes and the log says
why. "Next word now" in the menu shows one at once, "Next word in 5 s" after
five seconds, and `swift run Slipcard --show-after 5` five seconds after the
start.

To check the popup by hand, against a test profile (see "The test deck"
above), with a settings file that points at it, for example
`SLIPCARD_SETTINGS=/tmp/slipcard-test.json` holding
`{"ankiConnectUrl": "http://127.0.0.1:8765", "ankiConnectKey": null, "cardQuery": "deck:\"Slipcard Test\" is:due"}`:

1. `scripts/test-anki/test_deck.py reset`, start the app, then put another
   app in full screen (green button) and start typing in it.
2. Trigger a popup: "Next word in 5 s" in the menu bar item gives you time to
   switch back to the full-screen app, or start the app with `--show-after 5`.
3. The popup must appear in the middle of the screen over the full-screen app,
   on the same Space, and your typing must keep going into the other app. It
   shows the card's front in large type, as plain text (no HTML, no
   `[sound:…]`).
4. Show reveals the back (translation and example, line breaks kept) and the
   Again / Hard / Good / Easy buttons; the other app stays in full screen.
5. Good closes the popup, and `test_deck.py status` shows one due card less.
   Close closes it without answering (the count stays).
6. A long back wraps and ends with "…"; the popup stays on the screen.
7. The countdown: without touching anything, the Close button counts down
   from `countdownSeconds` ("Close (30)", "Close (29)" … once a second, 30 s
   by default; set `"countdownSeconds": 10` to wait less) and the popup
   closes at zero; `test_deck.py status` shows the same counts as before
   (the card was not answered), and the log says "closed by the countdown,
   not answered".
8. A new popup, and move the pointer over it: the button reads "Close" at
   once and the popup stays, also after the countdown would have ended. A
   new popup again, and press Show: the same. A pointer that only rests
   where the popup appears, without moving, does not stop the countdown.
9. The menu bar item: its menu shows the state first ("Connected to Anki",
   "10 cards due" right after a reset), then Next word now, Next word in 5 s,
   Pause for an hour, Quiet hours 22:00–08:00, Interval (5, 10, 15, 30,
   60 min, the current one checked), Open settings file, Quit Slipcard.
10. Pause for an hour: the menu reads "Paused until HH:MM" and offers Resume;
    no popup comes; Resume puts Pause back.
11. Quiet hours on (inside the range, or with a range set around now in the
    file): "Quiet hours until HH:MM"; the item is checked; off again.
12. Interval → 30 min: the check moves, and the settings file now says
    `"intervalMinutes": 30` (keys you added by hand are still there); quit
    and start the app again: still 30 min.
13. Open settings file opens it in the default editor (and creates it with
    the settings in effect if it was missing).
14. Stop Anki (or point the file at a dead port and restart): the icon greys
    out and the menu says "Anki is not running or not reachable"; start
    Anki and open the menu again: it is back to normal.
15. A broken settings file (a stray comma) and a restart: the menu starts
    with "Settings file is invalid: using the defaults"; changing the
    interval then says "Not saved: the settings file is invalid" and leaves
    the file as it was.

The app itself needs no permissions (Accessibility, notifications, Screen
Recording).

`Slipcard --tray-action interval:30` (or `quiet-hours`) runs a settings
action of the menu the same way the menu does and exits, for scripts and
tests: exit status 0 saved, 2 applied but not saved, 1 nothing to do.

## Linux (Pop!_OS 24.04, COSMIC)

The Linux app is a Rust crate in `linux/` (GTK 4 + gtk4-layer-shell). On
COSMIC (Wayland) the popup is a layer-shell surface on the overlay layer: the
compositor centres it above every window, does not tile it and gives it no
keyboard focus. The steps below are for Pop!_OS 24.04 and Ubuntu 24.04.

Packages, including Rust from the Ubuntu archive (the default `rustc` there is
too old for the GTK bindings):

```sh
sudo apt install build-essential pkg-config git meson ninja-build \
  libgtk-4-dev libwayland-dev wayland-protocols libgirepository1.0-dev valac \
  rustc-1.91 cargo-1.91
```

gtk4-layer-shell is not packaged for 24.04, so build its 1.3.0 release once
(it installs into `/usr/local`):

```sh
git clone --depth 1 --branch v1.3.0 https://github.com/wmww/gtk4-layer-shell
cd gtk4-layer-shell
meson setup build -Dexamples=false -Ddocs=false -Dtests=false \
  -Dintrospection=false -Dvapi=false
ninja -C build
sudo ninja -C build install
sudo ldconfig
```

Build, test and run (`RUSTC` points cargo at the matching compiler):

```sh
cd linux
export RUSTC=rustc-1.91
cargo-1.91 build
cargo-1.91 test                        # unit tests, no display needed
target/debug/slipcard --check          # OS, desktop, session, layer-shell support
target/debug/slipcard --show-after 5
```

It shows a popup every `intervalMinutes` with the next due card (the settings
are in `$XDG_CONFIG_HOME/slipcard/settings.json`, see "Settings"). While it
runs, Enter in its terminal is "next word now"; `q` and Enter, or Ctrl-C,
quits. There is no tray icon yet.

To check the popup by hand, against a test profile (see "The test deck"
above), with `SLIPCARD_SETTINGS` pointing at a settings file with that
profile's `ankiConnectUrl`, `ankiConnectKey` and
`"cardQuery": "deck:\"Slipcard Test\" is:due"`:

1. Run `--check`: on COSMIC it must say `wayland` and
   `layer shell supported by the compositor: true`. Then
   `scripts/test-anki/test_deck.py reset`.
2. Start the app with `--show-after 5`, switch to an editor (maximised, or in
   full screen) and keep typing; auto-tiling may be on.
3. The popup must appear in the middle of the screen above the editor, on an
   opaque background in the theme's colours, so the word stays readable over
   a busy window; no window may move or resize, and your typing must keep
   going into the editor. It shows the card's front in large type, as plain
   text.
4. Show reveals the back (translation and example) and the Again / Hard /
   Good / Easy buttons; clicking them must not move focus away from the
   editor for typing afterwards.
5. Good closes the popup and the terminal prints `popup: card … answered`;
   `test_deck.py status` shows one due card less. Close closes it without
   answering.
6. A long back wraps and ends with "…"; the popup stays on the screen.
7. The countdown: without touching anything, the Close button counts down
   from `countdownSeconds` once a second and the popup closes at zero; the
   terminal prints `popup: card … closed by the countdown, not answered` and
   `test_deck.py status` shows the same counts as before.
8. A new popup, and move the pointer over it: the button reads "Close" at
   once and the popup stays; the same after Show. A pointer that only rests
   where the popup appears, without moving, does not stop the countdown.
9. The tray: COSMIC's status area shows the Slipcard icon (a dictionary;
   the terminal says `tray: not available …` if the desktop has no
   StatusNotifierItem host). A click opens the menu: the state first
   ("Connected to Anki", "10 cards due" after a reset), then Next word now,
   Next word in 5 s, Pause for an hour, Quiet hours 22:00–08:00, Interval
   (5, 10, 15, 30, 60 min, the current one checked), Open settings file,
   Quit Slipcard; hovering the icon shows the state as a tooltip.
10. Pause, Resume, Quiet hours and Interval as on macOS (steps 10–12 there):
    the state lines follow, the terminal prints `tray: …`, the interval and
    the quiet-hours switch land in the settings file and survive a restart.
11. Open settings file opens it in the default text editor.
12. Stop Anki (or a dead port): the icon becomes the "offline" icon and the
    menu says "Anki is not running or not reachable"; back when Anki is.
13. A broken settings file: "Settings file is invalid: using the defaults"
    first in the menu; a change says "Not saved: …" and leaves the file.

`slipcard --tray-action interval:30` (or `quiet-hours`) runs a settings
action of the tray the same way the tray does and exits.

On a desktop without layer shell (GNOME, X11) the app warns and shows an
ordinary window instead, which the window manager may tile or focus.

## Before a pull request

- The platform's build and tests pass; if you touched `scripts/test-anki/`,
  `python3 -m unittest discover -s scripts/test-anki` passes too.
- You checked the popup by hand on your desktop if you touched window code,
  and said in the pull request which OS, version and desktop you used.
- Commits are signed off: `scripts/dco.sh origin/dev HEAD`.
