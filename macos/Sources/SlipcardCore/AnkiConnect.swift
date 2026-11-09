import Foundation

/// A card to show, as AnkiConnect's `cardsInfo` describes it.
public struct Card: Equatable, Sendable {
    public let id: Int64
    /// Anki's queue: 0 new, 1 learning, 2 review, 3 learning across days;
    /// below 0 suspended or buried.
    public let queue: Int
    /// For learning cards (queue 1) a Unix time in seconds; for review and
    /// day-learning cards a day number; for new cards a position.
    public let due: Int64
    /// The front, as Anki renders it (HTML, may start with a <style>).
    public let questionHTML: String
    /// The back without the front: Anki's rendered answer after
    /// `<hr id=answer>`.
    public let answerHTML: String
    /// What each grade would schedule, e.g. ["<10m", "2d", "3d", "4d"].
    public let nextReviews: [String]

    public init(
        id: Int64, queue: Int, due: Int64, questionHTML: String, answerHTML: String,
        nextReviews: [String] = []
    ) {
        self.id = id
        self.queue = queue
        self.due = due
        self.questionHTML = questionHTML
        self.answerHTML = answerHTML
        self.nextReviews = nextReviews
    }

    /// The part of Anki's rendered answer after the `<hr id=answer>` marker
    /// (the whole answer if a note type has no marker).
    public static func back(ofAnswer html: String) -> String {
        for marker in ["<hr id=answer>", "<hr id=\"answer\">", "<hr id='answer'>"] {
            if let range = html.range(of: marker) {
                return String(html[range.upperBound...])
            }
        }
        return html
    }
}

/// A grade, as AnkiConnect's `answerCards` takes it.
public enum Ease: Int, Sendable, CaseIterable {
    case again = 1, hard, good, easy
}

/// Every way talking to AnkiConnect can fail; each one is a state the tray
/// shows.
public enum AnkiConnectError: Error, Equatable, Sendable {
    /// Nothing answered at the address: Anki is not running, AnkiConnect is
    /// not installed, or the host is down.
    case notReachable(String)
    case timedOut
    /// AnkiConnect wants an API key and did not get the right one.
    case wrongKey
    /// AnkiConnect answered with an error.
    case anki(String)
    /// Something answered that is not AnkiConnect, or not as expected.
    case badResponse(String)
}

/// The connection state the tray shows.
public enum AnkiState: Equatable, Sendable, CustomStringConvertible {
    case unknown
    case connected
    case failed(AnkiConnectError)

    public init(_ error: Error) {
        self = .failed(error as? AnkiConnectError ?? .badResponse(String(describing: error)))
    }

    public var description: String {
        switch self {
        case .unknown: return "Not connected yet"
        case .connected: return "Connected to Anki"
        case .failed(.notReachable): return "Anki is not running or not reachable"
        case .failed(.timedOut): return "Anki did not answer in time"
        case .failed(.wrongKey): return "AnkiConnect refused the API key"
        case .failed(.anki(let message)): return "AnkiConnect: \(message)"
        case .failed(.badResponse): return "Unexpected answer from AnkiConnect"
        }
    }
}

public enum CardOrder {
    /// Learning cards first (the earliest due first), then learning across
    /// days and review cards (the most overdue first), then new cards in
    /// their order; suspended and buried cards never.
    public static func sorted(_ cards: [Card]) -> [Card] {
        func group(_ c: Card) -> Int {
            switch c.queue {
            case 1: return 0
            case 3: return 1
            case 2: return 2
            default: return 3
            }
        }
        return cards.filter { $0.queue >= 0 }.sorted {
            (group($0), $0.due, $0.id) < (group($1), $1.due, $1.id)
        }
    }
}

/// A client of AnkiConnect (API version 6).
public final class AnkiConnectClient: Sendable {
    public let url: URL
    public let key: String?
    public let timeout: TimeInterval
    private let session: URLSession

    public static let defaultTimeout: TimeInterval = 3

    public init(url: URL, key: String?, timeout: TimeInterval = AnkiConnectClient.defaultTimeout) {
        self.url = url
        self.key = key
        self.timeout = timeout
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = timeout
        config.timeoutIntervalForResource = timeout
        config.waitsForConnectivity = false
        config.httpMaximumConnectionsPerHost = 1
        session = URLSession(configuration: config)
    }

    /// The settings' address and key; nil if the address is not a URL.
    public convenience init?(settings: Settings, timeout: TimeInterval = AnkiConnectClient.defaultTimeout) {
        guard let url = URL(string: settings.ankiConnectURL) else { return nil }
        self.init(url: url, key: settings.ankiConnectKey, timeout: timeout)
    }

    public func activeProfile() async throws -> String {
        guard let name = try await call("getActiveProfile", [:]) as? String else {
            throw AnkiConnectError.badResponse("getActiveProfile: not a string")
        }
        return name
    }

    /// The card to show next for `query` (an Anki search), or nil when none
    /// is due.
    public func nextDueCard(query: String) async throws -> Card? {
        guard let ids = try await call("findCards", ["query": query]) as? [NSNumber] else {
            throw AnkiConnectError.badResponse("findCards: not a list of card ids")
        }
        var cards: [Card] = []
        for start in stride(from: 0, to: ids.count, by: 100) {
            let chunk = Array(ids[start..<min(start + 100, ids.count)])
            guard let infos = try await call("cardsInfo", ["cards": chunk]) as? [[String: Any]] else {
                throw AnkiConnectError.badResponse("cardsInfo: not a list of cards")
            }
            cards += infos.compactMap(Self.card)
        }
        return CardOrder.sorted(cards).first
    }

    /// Records a review of `cardID` with `ease`; Anki schedules it.
    public func answer(cardID: Int64, ease: Ease) async throws {
        let result = try await call(
            "answerCards", ["answers": [["cardId": NSNumber(value: cardID), "ease": ease.rawValue]]])
        guard let done = result as? [Bool], done == [true] else {
            throw AnkiConnectError.anki("the card was not answered (it may no longer exist)")
        }
    }

    static func card(_ info: [String: Any]) -> Card? {
        guard let id = (info["cardId"] as? NSNumber)?.int64Value,
            let queue = (info["queue"] as? NSNumber)?.intValue,
            let due = (info["due"] as? NSNumber)?.int64Value,
            let question = info["question"] as? String,
            let answer = info["answer"] as? String
        else { return nil }
        return Card(
            id: id, queue: queue, due: due, questionHTML: question,
            answerHTML: Card.back(ofAnswer: answer),
            nextReviews: info["nextReviews"] as? [String] ?? [])
    }

    /// Sends one action and returns its `result`, or throws what went wrong.
    func call(_ action: String, _ params: [String: Any]) async throws -> Any? {
        var body: [String: Any] = ["action": action, "version": 6, "params": params]
        if let key { body["key"] = key }
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        // AnkiConnect reads the key from the body; a proxy in front of an
        // AnkiConnect-compatible server can check this header instead.
        if let key { request.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization") }
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch let error as URLError {
            switch error.code {
            case .timedOut: throw AnkiConnectError.timedOut
            default: throw AnkiConnectError.notReachable(error.localizedDescription)
            }
        }
        if let http = response as? HTTPURLResponse, http.statusCode != 200 {
            throw AnkiConnectError.badResponse("HTTP \(http.statusCode)")
        }
        guard let reply = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
            reply.keys.contains("result") || reply.keys.contains("error")
        else { throw AnkiConnectError.badResponse("not an AnkiConnect reply") }
        if let message = reply["error"] as? String {
            if message.localizedCaseInsensitiveContains("api key") { throw AnkiConnectError.wrongKey }
            throw AnkiConnectError.anki(message)
        }
        let result = reply["result"]
        return result is NSNull ? nil : result
    }
}
