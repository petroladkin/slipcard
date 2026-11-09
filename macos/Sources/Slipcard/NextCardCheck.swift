import Foundation
import SlipcardCore

/// `Slipcard --next-card`: asks AnkiConnect for the card a popup would show
/// now, prints it or the connection state, and exits; no menu bar item, no
/// popup. Exit status: 0 a card is due, 3 none is due, 1 Anki failed.
@MainActor
func nextCardCheck() async -> Int32 {
    let url = SettingsStore.defaultURL()
    let load = SettingsStore.load(from: url)
    print("settings: \(url.path) (\(load.file))")
    for problem in load.problems { print("settings: \(problem)") }
    let s = load.settings
    guard let client = AnkiConnectClient(settings: s) else {
        print("state: the AnkiConnect address is not a URL: \(s.ankiConnectURL)")
        return 1
    }
    print("anki: \(s.ankiConnectURL) (\(s.ankiConnectKey == nil ? "no key" : "key set")), search: \(s.cardQuery)")
    do {
        let card = try await client.nextDueCard(query: s.cardQuery)
        print("state: \(AnkiState.connected)")
        guard let card else {
            print("next card: none due")
            return 3
        }
        print("next card: \(card.id), queue \(card.queue), due \(card.due), grades \(card.nextReviews)")
        print("front (raw): \(card.questionHTML.withoutStyle.prefix(120))")
        print("back (raw): \(card.answerHTML.prefix(120))")
        return 0
    } catch {
        print("state: \(AnkiState(error)) (\(error))")
        return 1
    }
}

extension String {
    /// The text after Anki's leading <style> block, for a one-line preview.
    var withoutStyle: String {
        guard let end = range(of: "</style>") else { return self }
        return String(self[end.upperBound...])
    }
}
