# Development

How to build, run and test vocab-popup on each platform.

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

`swift run` starts the app as a menu bar item (no Dock icon). To check the
popup's window behaviour by hand:

1. Put another app in full screen (green button) and start typing in it.
2. Trigger a popup ("Next word now" in the menu bar item).
3. The popup must appear over the full-screen app, on the same Space, and
   your typing must keep going into the other app.

Accessibility and notification permissions are not needed.

## Linux (Pop!_OS)

Not started yet. The Linux app comes after the popup proof of concept on the
target desktop, which decides between:

- **COSMIC (Pop!_OS 24.04, Wayland):** GTK4 + gtk4-layer-shell;
- **GNOME + Pop Shell (Pop!_OS 22.04):** GTK4 with a floating utility window.

To find which desktop you run:

```sh
echo "$XDG_CURRENT_DESKTOP $XDG_SESSION_TYPE"
grep PRETTY_NAME /etc/os-release
```

This section will list the packages and build commands once the language and
build system are chosen.

## Before a pull request

- The platform's build and tests pass.
- You checked the popup by hand on your desktop if you touched window code,
  and said in the pull request which OS, version and desktop you used.
- Commits are signed off: `scripts/dco.sh origin/dev HEAD`.
