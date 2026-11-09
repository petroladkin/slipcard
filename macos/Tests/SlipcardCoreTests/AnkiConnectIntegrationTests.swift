import Foundation
import Testing
@testable import SlipcardCore

/// Against a real test Anki, on demand only:
///
///   scripts/test-anki/test_deck.py reset
///   SLIPCARD_ANKI_INTEGRATION=1 SLIPCARD_ANKI_URL=... SLIPCARD_ANKI_KEY=... \
///     swift test --filter AnkiConnectIntegrationTests
///
/// Every test first checks that the active profile is "Slipcard Test" and
/// stops before anything else if it is not, so it never grades a card in a
/// real collection.
@Suite(.serialized, .enabled(if: ProcessInfo.processInfo.environment["SLIPCARD_ANKI_INTEGRATION"] == "1"))
struct AnkiConnectIntegrationTests {
    static let testProfile = "Slipcard Test"
    static let deck = "deck:\"Slipcard Test\""

    let env = ProcessInfo.processInfo.environment

    func guardedClient(key: String? = nil) async throws -> AnkiConnectClient {
        let url = try #require(URL(string: env["SLIPCARD_ANKI_URL"] ?? "http://127.0.0.1:8765"))
        let client = AnkiConnectClient(url: url, key: env["SLIPCARD_ANKI_KEY"])
        let profile = try await client.activeProfile()
        try #require(profile == Self.testProfile, "refusing: the active profile is \(profile)")
        return key.map { AnkiConnectClient(url: url, key: $0) } ?? client
    }

    @Test func nextDueCardIsFromTheTestDeckLearningFirst() async throws {
        let client = try await guardedClient()
        let card = try #require(try await client.nextDueCard(query: "\(Self.deck) is:due"))
        // After a reset the deck has 2 learning cards; they come first.
        #expect(card.queue == 1)
        #expect(!card.questionHTML.isEmpty)
        #expect(!card.answerHTML.contains("<hr id=answer>"))
    }

    @Test func answerIsRecordedAndMovesTheCard() async throws {
        let client = try await guardedClient()
        let card = try #require(try await client.nextDueCard(query: "\(Self.deck) is:review is:due"))
        let before = try await reviews(of: card.id, client)
        try await client.answer(cardID: card.id, ease: .good)
        let after = try await reviews(of: card.id, client)
        #expect(after.count == before.count + 1)
        #expect((after.last?["ease"] as? NSNumber)?.intValue == 3)
        let stillDue = try await client.call("findCards", ["query": "cid:\(card.id) is:due"]) as? [Any]
        #expect(stillDue?.isEmpty == true)
    }

    @Test func wrongKeyIsRefused() async throws {
        _ = try await guardedClient()
        try #require(env["SLIPCARD_ANKI_KEY"] != nil, "the test Anki has no API key")
        let wrong = try await guardedClient(key: "wrong")
        await #expect(throws: AnkiConnectError.wrongKey) { try await wrong.activeProfile() }
    }

    /// The popup's own path: the model the buttons call, with the real client.
    @MainActor @Test func gradeThroughThePopupModelIsRecorded() async throws {
        let client = try await guardedClient()
        let card = try #require(try await client.nextDueCard(query: "\(Self.deck) is:review is:due"))
        let before = try await reviews(of: card.id, client)
        let model = CardPopupModel(card: card, client: client)
        var closed = false
        model.onClose = { closed = true }
        #expect(!model.front.isEmpty && !model.front.contains("<"))
        model.reveal()
        await model.grade(.easy)
        #expect(closed)
        #expect(model.stage == .closed)
        let after = try await reviews(of: card.id, client)
        #expect(after.count == before.count + 1)
        #expect((after.last?["ease"] as? NSNumber)?.intValue == 4)
    }

    func reviews(of id: Int64, _ client: AnkiConnectClient) async throws -> [[String: Any]] {
        let result = try await client.call("getReviewsOfCards", ["cards": [String(id)]]) as? [String: Any]
        return result?[String(id)] as? [[String: Any]] ?? []
    }
}
