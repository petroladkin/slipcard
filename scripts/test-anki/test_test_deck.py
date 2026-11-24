"""Tests for test_deck.py against a stub AnkiConnect (no Anki needed).

Run: python3 -m unittest discover -s scripts/test-anki
"""
import http.server
import json
import threading
import unittest

import test_deck


class StubAnki:
    """A tiny AnkiConnect stand-in that records the actions it receives."""

    def __init__(self, profile, key=None):
        self.profile = profile
        self.key = key
        self.actions = []
        self.cards = []
        stub = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                stub.actions.append(body["action"])
                if stub.key is not None and body.get("key") != stub.key:
                    reply = {"result": None, "error": "valid api key must be provided"}
                else:
                    reply = {"result": stub.answer(body["action"], body.get("params", {})), "error": None}
                data = json.dumps(reply).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *args):
                pass

        self.server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_port}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def answer(self, action, params):
        if action == "getActiveProfile":
            return self.profile
        if action == "deckNames":
            return ["Default"]
        if action == "addNotes":
            self.cards = list(range(1000, 1000 + 2 * len(params["notes"])))
            return list(range(len(params["notes"])))
        if action == "findCards":
            return self.cards
        return None

    def close(self):
        self.server.shutdown()
        self.server.server_close()


class GuardTest(unittest.TestCase):
    def test_refuses_another_profile_before_any_other_request(self):
        stub = StubAnki(profile="User 1")
        self.addCleanup(stub.close)
        for command in ("status", "reset"):
            with self.subTest(command=command):
                stub.actions.clear()
                self.assertEqual(test_deck.main([command, "--url", stub.url]), 1)
                self.assertEqual(stub.actions, ["getActiveProfile"])

    def test_works_on_the_test_profile(self):
        stub = StubAnki(profile=test_deck.TEST_PROFILE)
        self.addCleanup(stub.close)
        test_deck.main(["reset", "--url", stub.url])
        self.assertEqual(stub.actions[0], "getActiveProfile")
        for action in ("createDeck", "addNotes", "setDueDate", "answerCards"):
            self.assertIn(action, stub.actions)

    def test_sends_the_api_key(self):
        stub = StubAnki(profile=test_deck.TEST_PROFILE, key="k")
        self.addCleanup(stub.close)
        self.assertEqual(test_deck.main(["status", "--url", stub.url]), 1)
        stub.actions.clear()
        self.assertEqual(test_deck.main(["status", "--url", stub.url, "--key", "k"]), 0)

    def test_unreachable_anki_is_an_error_not_a_crash(self):
        self.assertEqual(test_deck.main(["status", "--url", "http://127.0.0.1:9"]), 1)


if __name__ == "__main__":
    unittest.main()
