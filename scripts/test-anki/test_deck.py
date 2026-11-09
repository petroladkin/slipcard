#!/usr/bin/env python3
"""Fills, resets and reports the test deck of a development Anki.

Grading a card changes its schedule, so development and integration tests
must never touch a real collection. This tool talks to AnkiConnect and works
only on the profile named TEST_PROFILE: every command asks AnkiConnect for
the active profile first and stops before any other request if it is not
the test profile.

  test_deck.py status   counts of the test deck's cards by state
  test_deck.py reset    delete the test deck and build it again in a known
                        state (see EXPECTED), with made-up words

The AnkiConnect address and API key come from SLIPCARD_ANKI_URL (default
http://127.0.0.1:8765) and SLIPCARD_ANKI_KEY (optional), or --url / --key.
Only the Python standard library is used.
"""
import argparse
import json
import os
import sys
import urllib.error
import urllib.request

TEST_PROFILE = "Slipcard Test"
DECK = "Slipcard Test"
MODEL = "Basic (and reversed card)"
TAG = "slipcard-test"

# (front, back). Each note makes two cards (both directions). Some backs
# carry the markup Anki cards usually have, so the popup's HTML-to-text
# conversion has something to chew on.
WORDS = [
    ("serendipity", "a happy accident<br><i>Finding this café was pure serendipity.</i>"),
    ("ephemeral", "lasting a very short time<br><i>Fame on the internet is ephemeral.</i>"),
    ("meticulous", "very careful about small details<br><i>She keeps meticulous notes.</i>"),
    ("ubiquitous", "found everywhere<br><i>Phones are ubiquitous now.</i>"),
    ("candid", "honest and direct<br><i>Thanks for your candid answer.</i>"),
    ("resilient", "able to recover quickly<br><i>Children are remarkably resilient.</i>"),
    ("tedious", "long and boring<br><i>Filling in forms is tedious.</i>"),
    ("frugal", "careful with money<br><i>A frugal meal of bread and soup.</i>"),
    ("<b>reluctant</b>", "not willing to do something<div><i>He was reluctant to leave.</i></div>"),
    ("&quot;brisk&quot; walk", "quick and energetic<br><i>We went for a brisk walk.</i>"),
    ("lucid [sound:lucid.mp3]", "clear and easy to understand<br><i>A lucid explanation.</i>"),
    ("verbose", 'using more words than needed<br><img src="verbose.png"><br><i>The manual is verbose.</i>'),
]

# The state reset() leaves the deck in, in cards (2 per note).
EXPECTED = {"review_due": 8, "review_later": 4, "learning": 2, "new": 10}

QUERIES = {
    "review_due": f'deck:"{DECK}" is:review is:due',
    "review_later": f'deck:"{DECK}" is:review -is:due',
    "learning": f'deck:"{DECK}" is:learn',
    "new": f'deck:"{DECK}" is:new',
}


class AnkiError(Exception):
    pass


class AnkiConnect:
    def __init__(self, url, key=None, timeout=10.0):
        self.url = url
        self.key = key
        self.timeout = timeout

    def call(self, action, **params):
        body = {"action": action, "version": 6, "params": params}
        if self.key:
            body["key"] = self.key
        req = urllib.request.Request(
            self.url, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}
        )
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                reply = json.load(resp)
        except (urllib.error.URLError, OSError) as e:
            raise AnkiError(f"AnkiConnect at {self.url} is not reachable: {e}") from e
        if reply.get("error"):
            raise AnkiError(f"{action}: {reply['error']}")
        return reply.get("result")


def guard(anki):
    """Stops unless the active profile is the test profile."""
    active = anki.call("getActiveProfile")
    if active != TEST_PROFILE:
        raise AnkiError(
            f"refusing: the active Anki profile is {active!r}, not {TEST_PROFILE!r}; "
            "this tool only works on the test profile"
        )


def status(anki):
    guard(anki)
    return {name: len(anki.call("findCards", query=q)) for name, q in QUERIES.items()}


def reset(anki):
    guard(anki)
    if DECK in anki.call("deckNames"):
        anki.call("deleteDecks", decks=[DECK], cardsToo=True)
    anki.call("createDeck", deck=DECK)
    notes = [
        {
            "deckName": DECK,
            "modelName": MODEL,
            "fields": {"Front": front, "Back": back},
            "tags": [TAG],
            "options": {"allowDuplicate": True},
        }
        for front, back in WORDS
    ]
    ids = anki.call("addNotes", notes=notes)
    if None in ids:
        raise AnkiError("addNotes: some notes were not added")
    cards = sorted(anki.call("findCards", query=f'deck:"{DECK}"'))
    due = cards[: EXPECTED["review_due"]]
    later = cards[len(due) : len(due) + EXPECTED["review_later"]]
    learn = cards[len(due) + len(later) : len(due) + len(later) + EXPECTED["learning"]]
    anki.call("setDueDate", cards=due, days="0")
    anki.call("setDueDate", cards=later, days="7")
    # Again on a new card moves it into its first learning step.
    anki.call("answerCards", answers=[{"cardId": c, "ease": 1} for c in learn])
    return status(anki)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("command", choices=["status", "reset"])
    p.add_argument("--url", default=os.environ.get("SLIPCARD_ANKI_URL", "http://127.0.0.1:8765"))
    p.add_argument("--key", default=os.environ.get("SLIPCARD_ANKI_KEY"))
    args = p.parse_args(argv)
    anki = AnkiConnect(args.url, args.key)
    try:
        counts = status(anki) if args.command == "status" else reset(anki)
    except AnkiError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    print(json.dumps(counts))
    if args.command == "reset" and counts != EXPECTED:
        print(f"error: expected {json.dumps(EXPECTED)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
