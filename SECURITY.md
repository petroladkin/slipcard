# Security policy

## Reporting a vulnerability

Please do not open a public issue for a security problem. Report it
privately through GitHub: the repository's **Security** tab →
**Report a vulnerability**. Say what is affected, how to reproduce it and
what an attacker gains.

The aim is to answer within a week. A confirmed problem is fixed on `dev` and
in a patch release of the supported version, and the advisory is published
with the fix; you are credited unless you ask not to be.

## Supported versions

Nothing is released yet. Once releases exist, only the newest minor version
gets fixes before 1.0.

## In scope

- vocab-popup sending requests to anything but the configured AnkiConnect
  address, or sending card content anywhere else.
- A way for another local process or a web page to make vocab-popup answer,
  change or read cards through it.
- The popup taking keyboard focus and so capturing keystrokes meant for
  another app (for example a password typed while it appears).
- Settings or logs leaking an AnkiConnect API key.

## Not a vulnerability

- AnkiConnect itself listens on `127.0.0.1:8765` and, without an API key,
  answers any local process. That is AnkiConnect's design; set its `apiKey`
  option if other local software is a concern, and report AnkiConnect issues
  to [its project](https://git.foosoft.net/alex/anki-connect).
- Card content is shown on screen, over other apps, by design. Use pause or
  quiet hours while sharing the screen.
