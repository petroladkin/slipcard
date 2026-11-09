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
Add), with a small test deck. Grading cards from a test build changes their
schedule, and you do not want that in your real collection. Switch back to
your own profile when you are done.

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
"Quit Slipcard" in its menu stops it. `swift run Slipcard --show-after 5`
also shows a popup five seconds after the start. To check the popup's window
behaviour by hand:

1. Start the app, then put another app in full screen (green button) and start
   typing in it.
2. Trigger a popup: "Next word in 5 s" in the menu bar item gives you time to
   switch back to the full-screen app, or start the app with `--show-after 5`.
3. The popup must appear in the middle of the screen over the full-screen app,
   on the same Space, and your typing must keep going into the other app.
4. Show and Close must work with a click, and the other app must stay in full
   screen.

The app itself needs no permissions (Accessibility, notifications, Screen
Recording).

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

While it runs, Enter in its terminal shows another popup; `q` and Enter, or
Ctrl-C, quits. There is no tray icon yet.

To check the popup's window behaviour by hand:

1. Run `--check`: on COSMIC it must say `wayland` and
   `layer shell supported by the compositor: true`.
2. Start the app with `--show-after 5`, switch to an editor (maximised, or in
   full screen) and keep typing; auto-tiling may be on.
3. The popup must appear in the middle of the screen above the editor, on an
   opaque background in the theme's colours, so the word stays readable over
   a busy window; no window may move or resize, and your typing must keep
   going into the editor.
4. Show and Close must work with a click; Close removes the popup.

On a desktop without layer shell (GNOME, X11) the app warns and shows an
ordinary window instead, which the window manager may tile or focus.

## Before a pull request

- The platform's build and tests pass.
- You checked the popup by hand on your desktop if you touched window code,
  and said in the pull request which OS, version and desktop you used.
- Commits are signed off: `scripts/dco.sh origin/dev HEAD`.
