# Slipcard

Guidance for AI coding agents (and a concise orientation for humans).

Slipcard shows Anki vocabulary cards in a popup on a timer:
a tray / menu bar icon, and every N minutes a card appears in the middle of
the screen, on top of everything and without taking focus. The user reveals
the answer and grades it (Again / Hard / Good / Easy) or lets the popup close
itself. Scheduling and sync stay in Anki, reached through AnkiConnect.
Targets: Pop!_OS and macOS. Open source.

## Problem

Learning words in Anki needs repetitions through the day, and the existing
options fail on the target desktops:

- System notifications on Pop!_OS last about 3 seconds, too short to answer.
- A plain window is tiled on Pop!_OS (it splits the screen) and does not
  appear over full-screen apps on macOS.
- The Ruzu Pop-ups add-on (an ordinary Anki window) has both problems, mixed
  reliability reports and no auto-close.

## MVP scope

- Tray / menu bar icon: pause for an hour, quiet hours, "next word now",
  interval.
- Every N minutes a popup appears on top of everything, without focus (it
  never steals typing): word → "Show" → translation and example → Again /
  Hard / Good / Easy.
- "Close (30)" countdown: auto-close is not an answer; the card stays due.
- Data and schedule from Anki via AnkiConnect (`findCards` → `cardsInfo` →
  `answerCards`), so AnkiWeb sync, mobile apps and the review schedule keep
  working. Anki runs in the background.
- v0.2: do not show during a call, screen sharing or a presentation.

## Out of scope

Own spaced-repetition algorithm or sync (while Anki is there), a card editor,
Windows, mobile apps (AnkiDroid / AnkiMobile already cover them).

## Stack

Native on each platform, no cross-platform UI framework:

- **macOS:** Swift, `NSStatusItem` + `NSPanel` (non-activating, floating,
  `canJoinAllSpaces` + `fullScreenAuxiliary`), so the panel shows over
  full-screen apps without switching Space.
- **Pop!_OS, COSMIC (Wayland):** the popup as a layer-shell surface (overlay):
  not tiled, centred, on top. GTK4 + gtk4-layer-shell; tray via
  StatusNotifierItem.
- **Pop!_OS, GNOME + Pop Shell (22.04):** no layer-shell; a utility/dialog
  window or a Pop Shell "Floating Window Exceptions" rule; tray via
  AppIndicator.

The Linux target is Pop!_OS 24.04 COSMIC (layer-shell). The GNOME + Pop
Shell path is not built: without layer-shell the app warns and shows an
ordinary window.

## Risks

- Wayland and tiling: popup behaviour depends on the compositor, so a popup
  prototype on Pop!_OS is the first step.
- Depends on Anki and AnkiConnect running.
- Intrusiveness: a popup that gets in the way is switched off within a week,
  hence pause, quiet hours and no focus.

## Open questions

1. Word source format and card direction; is pronunciation (TTS) needed.

Settled: the Linux target is Pop!_OS 24.04 COSMIC; the defaults are a
15-minute interval and a 30-second countdown (`docs/settings.example.json`).

## Where to read more

- `README.md`: what the app does and how to build it.
- `docs/architecture.md`: how the pieces fit on each platform.
- `docs/development.md`: building, testing and the workflow.
- `CONTRIBUTING.md`: how to contribute.

## Language

Everything in this repository (code, comments, commits, branches, PRs, docs)
is in English.

## Working rules

- Gitflow: `main` + `dev` + `feature/*` + `release/*` + `fix/*` + `hotfix/*`.
  No direct commits to `dev` or `main`: only through a branch and a PR.
  Features and fixes branch off `dev`; `hotfix/*` off `main`.
- This is a public repository. Commits are Petro's only (`git commit -s`);
  no Claude / Anthropic co-author lines, "Generated with" footers or similar
  in commits, PRs, issues, releases or code. Before a push:
  `git log <base>..HEAD --format=%B | grep -ciE "co-authored|claude|anthropic"`
  must print 0 (the file name `CLAUDE.md` is not authorship).
- No personal context in this repository: machine paths, private repository
  names and task IDs stay out of files, commits, branches and PRs.

<!-- BACKLOG.MD GUIDELINES START -->
<!-- backlog.md-instructions-version: 1.53.0 -->
<CRITICAL_INSTRUCTION>

## Backlog.md Workflow

This project uses Backlog.md for task and project management.

**At the beginning of each conversation in this project, run `backlog instructions overview` before answering or taking action. Re-read it only if you have not read it yet in the current conversation.**

Use the overview to decide whether to search, read, create, or update Backlog tasks.

Before task lifecycle actions, read the matching detailed guide:
- `backlog instructions task-creation` before creating or splitting tasks
- `backlog instructions task-execution` before planning, changing status or assignee, adding a plan or implementation notes, or implementing task work
- `backlog instructions task-finalization` before checking acceptance criteria, writing final summaries, or moving tasks to terminal statuses

Use `backlog <command> --help` before running unfamiliar commands. Help shows options, fields, and examples.

Do not edit Backlog task, draft, document, decision, or milestone markdown files directly. Use the `backlog` CLI so metadata, relationships, and history stay consistent.

</CRITICAL_INSTRUCTION>
<!-- BACKLOG.MD GUIDELINES END -->

## Tracking work

Open work is tracked in Backlog.md outside this repository; GitHub issues are
not used. `docs/backlog` is a local, gitignored link to the maintainer's
tasks: nothing in the repository may depend on it.
