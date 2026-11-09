# Slipcard

Anki vocabulary cards in a timed popup from the tray: on top of full-screen
apps, never tiled, never stealing focus. Pop!_OS and macOS.

**Status: early development.** Nothing is released yet; the first step is a
proof of concept of the popup on each platform.

## Why

Spaced repetition works best with a few reviews spread through the day, but
the usual ways to get a card in front of you fail on a working desktop:

- system notifications on Pop!_OS disappear after about three seconds;
- an ordinary window is tiled on Pop!_OS (it splits the screen) and does not
  show over full-screen apps on macOS;
- a window that takes keyboard focus eats whatever you were typing.

Slipcard shows one card every N minutes in the middle of the screen, on top
of everything and without taking focus. You reveal the answer and grade it
(Again / Hard / Good / Easy), or ignore it and the popup closes itself after a
countdown; closing is not an answer, so the card stays due.

## How it works

Anki stays the source of cards and the scheduler. Slipcard talks to a
running Anki through the [AnkiConnect](https://foosoft.net/projects/anki-connect/)
add-on: it asks for due cards, shows one, and sends your grade back. AnkiWeb
sync, the mobile apps and the review schedule keep working as before.

Each platform has its own native app; there is no cross-platform UI toolkit.
See [docs/architecture.md](docs/architecture.md).

| Platform | App | Popup |
|---|---|---|
| macOS 14+ | Swift, `macos/` | non-activating `NSPanel` on all Spaces, over full-screen apps |
| Pop!_OS (COSMIC, Wayland) | GTK4, `linux/` (planned) | layer-shell overlay surface |
| Pop!_OS (GNOME + Pop Shell) | GTK4, `linux/` (planned) | floating utility window |

## Planned features (MVP)

- Tray / menu bar icon: pause for an hour, quiet hours, "next word now",
  interval.
- A popup every N minutes: word → Show → translation and example → grade.
- A "Close (30)" countdown that does not count as an answer.
- Later: stay hidden during a call, screen sharing or a presentation.

Not planned: an own scheduling algorithm or sync, a card editor, Windows,
mobile apps (AnkiDroid and AnkiMobile already exist).

## Building

See [docs/development.md](docs/development.md) for each platform.

## Contributing

Issues and pull requests are welcome; please read
[CONTRIBUTING.md](CONTRIBUTING.md) first. Security problems are reported
privately, see [SECURITY.md](SECURITY.md). Everyone taking part follows the
[Code of Conduct](CODE_OF_CONDUCT.md).

## License

[Apache License 2.0](LICENSE). Anki and AnkiConnect are separate projects with
their own licenses; Slipcard only talks to them over AnkiConnect's local
HTTP API and does not include their code.
