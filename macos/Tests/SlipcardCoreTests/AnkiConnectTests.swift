import Foundation
import Testing
@testable import SlipcardCore

/// The recorded AnkiConnect replies in testdata/ankiconnect, shared with the
/// Linux tests.
func recorded(_ name: String) throws -> Data {
    let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent()
    return try Data(contentsOf: root.appendingPathComponent("testdata/ankiconnect/\(name).json"))
}

@Suite struct AnkiConnectTests {
    func client(_ server: StubAnkiServer, key: String? = nil, timeout: TimeInterval = 3) -> AnkiConnectClient {
        AnkiConnectClient(url: server.url, key: key, timeout: timeout)
    }

    func recordedServer() throws -> StubAnkiServer {
        try StubAnkiServer(replies: [
            "findCards": try recorded("findCards"),
            "cardsInfo": try recorded("cardsInfo"),
            "answerCards": try recorded("answerCards"),
            "getActiveProfile": StubAnkiServer.reply(result: "Slipcard Test"),
        ])
    }

    // MARK: next due card

    @Test func nextDueCardReadsFindCardsThenCardsInfo() async throws {
        let server = try recordedServer()
        let card = try await client(server).nextDueCard(query: "deck:\"Slipcard Test\" is:due")
        #expect(server.actions == ["findCards", "cardsInfo"])
        #expect((server.requests[0]["params"] as? [String: Any])?["query"] as? String
            == "deck:\"Slipcard Test\" is:due")
        // The recorded cards are a learning, a review and a new card: the
        // learning card comes first.
        let c = try #require(card)
        #expect(c.queue == 1)
        #expect(c.questionHTML.hasSuffix("tedious"))
        #expect(c.answerHTML == "\n\nlong and boring<br><i>Filling in forms is tedious.</i>")
        #expect(c.nextReviews.count == 4)
    }

    @Test func noDueCardIsNil() async throws {
        let server = try StubAnkiServer(replies: ["findCards": StubAnkiServer.reply(result: [Int]())])
        #expect(try await client(server).nextDueCard(query: "is:due") == nil)
        #expect(server.actions == ["findCards"])
    }

    @Test func cardsInfoIsAskedInChunksOf100() async throws {
        let ids = Array(1...250)
        let server = try StubAnkiServer(replies: [
            "findCards": StubAnkiServer.reply(result: ids),
            "cardsInfo": StubAnkiServer.reply(result: [[String: Any]]()),
        ])
        _ = try await client(server).nextDueCard(query: "is:due")
        let sizes = server.requests.dropFirst().map {
            (($0["params"] as? [String: Any])?["cards"] as? [Any])?.count ?? 0
        }
        #expect(sizes == [100, 100, 50])
    }

    @Test func orderIsLearningThenMostOverdueThenNew() {
        func card(_ id: Int64, queue: Int, due: Int64) -> Card {
            Card(id: id, queue: queue, due: due, questionHTML: "", answerHTML: "")
        }
        let cards = [
            card(1, queue: 0, due: 1),  // new
            card(2, queue: 2, due: 40),  // review, due day 40
            card(3, queue: 1, due: 1_800_000_100),  // learning, later
            card(4, queue: 2, due: 12),  // review, more overdue
            card(5, queue: -1, due: 0),  // suspended
            card(6, queue: 1, due: 1_800_000_000),  // learning, sooner
            card(7, queue: 3, due: 50),  // learning across days
            card(8, queue: -2, due: 0),  // buried
        ]
        #expect(CardOrder.sorted(cards).map(\.id) == [6, 3, 7, 4, 2, 1])
    }

    @Test func backIsTheAnswerAfterTheMarker() {
        #expect(Card.back(ofAnswer: "front\n\n<hr id=answer>\n\nback") == "\n\nback")
        #expect(Card.back(ofAnswer: "front<hr id=\"answer\">back") == "back")
        #expect(Card.back(ofAnswer: "no marker") == "no marker")
    }

    // MARK: answering

    @Test func answerSendsCardIdAndEase() async throws {
        let server = try recordedServer()
        try await client(server).answer(cardID: 1_794_264_260_702, ease: .good)
        #expect(server.actions == ["answerCards"])
        let answers = (server.requests[0]["params"] as? [String: Any])?["answers"] as? [[String: Any]]
        #expect((answers?[0]["cardId"] as? NSNumber)?.int64Value == 1_794_264_260_702)
        #expect((answers?[0]["ease"] as? NSNumber)?.intValue == 3)
    }

    @Test func answerThatAnkiDidNotRecordIsAnError() async throws {
        let server = try StubAnkiServer(replies: ["answerCards": StubAnkiServer.reply(result: [false])])
        await #expect(throws: AnkiConnectError.anki("the card was not answered (it may no longer exist)")) {
            try await client(server).answer(cardID: 1, ease: .again)
        }
    }

    @Test func easeValuesAreAnkis() {
        #expect(Ease.allCases.map(\.rawValue) == [1, 2, 3, 4])
    }

    // MARK: key

    @Test func keyIsSentWhenSet() async throws {
        let server = try recordedServer()
        _ = try await client(server, key: "k").activeProfile()
        _ = try await client(server, key: nil).activeProfile()
        #expect(server.requests[0]["key"] as? String == "k")
        #expect(server.requests[1]["key"] == nil)
        #expect(server.requests.allSatisfy { ($0["version"] as? Int) == 6 })
    }

    @Test func keyIsAlsoSentAsABearerHeader() async throws {
        let server = try recordedServer()
        _ = try await client(server, key: "k").activeProfile()
        _ = try await client(server, key: nil).activeProfile()
        #expect(server.authorizations == ["Bearer k", nil])
    }

    @Test func clientFromSettings() throws {
        var s = Settings.defaults
        s.ankiConnectKey = "k"
        let c = try #require(AnkiConnectClient(settings: s))
        #expect(c.url.absoluteString == "http://127.0.0.1:8765")
        #expect(c.key == "k")
        #expect(c.timeout == 3)
    }

    // MARK: failures, each its own state

    @Test func wrongKey() async throws {
        let server = try StubAnkiServer(replies: ["findCards": try recorded("error-no-key")])
        await #expect(throws: AnkiConnectError.wrongKey) {
            try await client(server).nextDueCard(query: "is:due")
        }
    }

    @Test func ankiError() async throws {
        let server = try StubAnkiServer(replies: ["findCards": try recorded("error-unsupported-action")])
        await #expect(throws: AnkiConnectError.anki("unsupported action")) {
            try await client(server).nextDueCard(query: "is:due")
        }
    }

    @Test func nothingListening() async throws {
        // Port 9 (discard) has no listener on a development machine.
        let c = AnkiConnectClient(url: URL(string: "http://127.0.0.1:9")!, key: nil)
        do {
            _ = try await c.nextDueCard(query: "is:due")
            Issue.record("expected an error")
        } catch let error as AnkiConnectError {
            guard case .notReachable = error else { Issue.record("got \(error)"); return }
        }
    }

    @Test func noAnswerTimesOut() async throws {
        let server = try StubAnkiServer(behaviour: .silent)
        let start = Date()
        await #expect(throws: AnkiConnectError.timedOut) {
            try await client(server, timeout: 0.5).nextDueCard(query: "is:due")
        }
        #expect(Date().timeIntervalSince(start) < 3)
    }

    @Test func httpErrorIsABadResponse() async throws {
        let server = try StubAnkiServer(behaviour: .status(500))
        await #expect(throws: AnkiConnectError.badResponse("HTTP 500")) {
            try await client(server).nextDueCard(query: "is:due")
        }
    }

    @Test func notAnAnkiConnectReply() async throws {
        let server = try StubAnkiServer(replies: ["findCards": Data("{\"hello\": 1}".utf8)])
        await #expect(throws: AnkiConnectError.badResponse("not an AnkiConnect reply")) {
            try await client(server).nextDueCard(query: "is:due")
        }
    }

    @Test func statesReadForTheTray() {
        #expect(AnkiState.connected.description == "Connected to Anki")
        #expect(AnkiState(AnkiConnectError.notReachable("x")).description == "Anki is not running or not reachable")
        #expect(AnkiState(AnkiConnectError.timedOut).description == "Anki did not answer in time")
        #expect(AnkiState(AnkiConnectError.wrongKey).description == "AnkiConnect refused the API key")
        #expect(AnkiState(AnkiConnectError.anki("boom")).description == "AnkiConnect: boom")
        #expect(AnkiState(AnkiConnectError.badResponse("x")).description == "Unexpected answer from AnkiConnect")
    }
}
