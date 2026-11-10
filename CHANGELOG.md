# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## Unreleased

- Project set up: license (Apache-2.0), contributor documentation.
- Renamed from vocab-popup to Slipcard.
- Settings file (`settings.json`) shared by the macOS and Linux apps.
- AnkiConnect client on both platforms: the next due card, answering it, and a state for every kind of failure; `--next-card` prints what a popup would show.
- Scheduler on both platforms: the interval without stacking, pause for an hour, quiet hours across midnight, next word now, and no burst of popups after sleep.
- The popup shows the next due card from Anki (front, Show, back) and sends Again / Hard / Good / Easy.
- The Close button counts down (`countdownSeconds`) and closes the popup without an answer; moving the pointer over the popup or pressing Show stops it.
- The AnkiConnect key is also sent as `Authorization: Bearer`, so a proxy can guard an AnkiConnect-compatible server.
- Daily use: `scripts/macos-bundle.sh` builds an ad-hoc signed `Slipcard.app` and installs it; `scripts/linux-install.sh` installs the binary and a desktop entry; "Start at login" in the tray on both.
- Tray on both platforms (macOS menu bar item, Linux StatusNotifierItem): next word now, pause for an hour, quiet hours, interval (saved to the settings), open the settings file, quit; it shows the connection, the due cards, the pause, quiet hours and an invalid settings file, and the icon changes when Anki is not reachable.
