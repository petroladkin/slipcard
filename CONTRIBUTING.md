# Contributing to Slipcard

Thanks for looking. The project is young (before 0.1) and the shape of the
code is still settling, so **open an issue before a pull request** and say
what problem the change solves. That costs less than a pull request that
cannot be merged. A security problem is reported privately instead
([SECURITY.md](SECURITY.md)).

Good first contributions: testing the popup on a desktop we do not have
(another Linux compositor, an older macOS), and reporting exactly what you
saw. Popup behaviour depends on the window manager, so a precise report is
worth as much as code.

## Getting the code running

[docs/development.md](docs/development.md) lists what each platform needs and
how to build, run and test it. [docs/architecture.md](docs/architecture.md)
explains how the pieces fit, and what is deliberately left out.

Enable the repository's git hooks once per clone:

```sh
git config core.hooksPath scripts/hooks
```

They refuse commits on `dev` and `main` (see Branches).

## Sign your commits (DCO)

Every commit needs a `Signed-off-by` line, which `git commit -s` adds:

```
Signed-off-by: Your Name <you@example.com>
```

It means you agree to the [Developer Certificate of
Origin](https://developercertificate.org/): you wrote the change, or have the
right to submit it, and you submit it under this project's license
([Apache-2.0](LICENSE)). The name and email must match the commit's author.
To fix a missing sign-off: `git commit --amend -s`, or
`git rebase --signoff <base>` for several commits. `scripts/dco.sh <base>
<head>` checks a range, the same way CI does (see below).

## Continuous integration

Every pull request into `dev` or `main` runs the workflow
[`.github/workflows/ci.yml`](.github/workflows/ci.yml). Its three jobs must
pass before the pull request can be merged:

| Job | Runner | What it runs |
|---|---|---|
| `macos` | macOS 15 | `swift build` and `swift test` in `macos/` |
| `linux` | Ubuntu 24.04 | the packages and the gtk4-layer-shell 1.3.0 build from [docs/development.md](docs/development.md), then `cargo build --locked` and `cargo test --locked` in `linux/` with `rustc-1.91` |
| `dco` | Ubuntu 24.04 | `scripts/dco.sh <base> <head>` over the pull request's commits |

The gtk4-layer-shell build and cargo's downloads and `target/` are cached
between runs. No job needs a display or an Anki: the integration tests
against a test Anki are not part of CI.

To run the same checks before you push (the Linux setup once, as in
[docs/development.md](docs/development.md)):

```sh
(cd macos && swift build && swift test)                      # macOS
(cd linux && RUSTC=rustc-1.91 cargo-1.91 build --locked \
  && RUSTC=rustc-1.91 cargo-1.91 test --locked)              # Linux
scripts/dco.sh origin/dev HEAD                               # sign-offs
```

## Branches

The project follows gitflow:

- Branch from `dev`, target `dev`. `main` holds released versions: it only
  receives release branches and hotfixes, and every merge into it is tagged
  (`vX.Y.Z`).
- Branch names: `feature/<name>`, `fix/<name>`; `release/x.y` and
  `hotfix/<name>` are for maintainers.
- A fix for a released version goes on `hotfix/<name>` from `main`, is
  released as the next patch version and merged back into `dev`.
- One branch, one concern. A fix and a refactor are two pull requests.
- Nobody commits to `dev` or `main` directly, maintainers included.

## Commits

- [Conventional Commits](https://www.conventionalcommits.org/): `feat:`,
  `fix:`, `docs:`, `chore:`, `refactor:`, `test:`. A scope names the
  platform when the change is limited to one: `feat(macos): ...`,
  `fix(linux): ...`. The body says why, not what.
- Code, comments, identifiers, commit messages and docs are in English.
- Tests go with the change (see each platform's section in
  [docs/development.md](docs/development.md)).

## Code

- **Native per platform.** The macOS app is Swift with AppKit; the Linux app
  is GTK4. Please do not propose a cross-platform UI toolkit (Electron, Qt,
  Flutter, Tauri): the whole point is window behaviour that only the native
  APIs give (non-activating panels, layer-shell surfaces).
- **The popup never takes keyboard focus** unless the user clicks it. A change
  that makes it steal focus, even in an edge case, is a bug.
- **Anki owns the data.** Slipcard reads cards and sends grades through
  AnkiConnect and stores nothing about cards itself. Settings (interval, quiet
  hours) are the only local state.
- **Fail visibly.** When Anki or AnkiConnect is not reachable, the tray says
  so; nothing fails silently.
- Keep logic that does not touch the window system (scheduling the next
  popup, quiet hours, the AnkiConnect client) in plain code that unit tests
  can run without a display.

## Documentation

A change that changes what a user sees changes the page that describes it, in
the same pull request: the README for features, `docs/development.md` for
build steps, `docs/architecture.md` for structure. Add a line under
"Unreleased" in [CHANGELOG.md](CHANGELOG.md) for anything a user would
notice.

## Reporting a problem

Bugs go to the issue tracker: your OS and desktop (for Linux: distribution,
version, compositor, Wayland or X11), the Slipcard and Anki versions, what
you did, what you saw, what you expected. Please do not report security
problems in public issues; see [SECURITY.md](SECURITY.md).
