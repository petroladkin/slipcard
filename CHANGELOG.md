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
