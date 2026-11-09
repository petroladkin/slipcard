import Foundation

/// A time of day, "HH:MM" in the settings file.
public struct ClockTime: Equatable, Sendable, CustomStringConvertible {
    public let hour: Int
    public let minute: Int

    public init?(hour: Int, minute: Int) {
        guard (0...23).contains(hour), (0...59).contains(minute) else { return nil }
        self.hour = hour
        self.minute = minute
    }

    /// Parses "HH:MM" (24-hour clock, two digits each).
    public init?(_ text: String) {
        let parts = text.split(separator: ":", omittingEmptySubsequences: false)
        guard parts.count == 2, parts.allSatisfy({ $0.count == 2 && $0.allSatisfy { $0.isASCII && $0.isNumber } }),
            let hour = Int(parts[0]), let minute = Int(parts[1])
        else { return nil }
        self.init(hour: hour, minute: minute)
    }

    public var description: String { String(format: "%02d:%02d", hour, minute) }
}

/// Slipcard's settings. The file may hold any subset of the keys; a missing
/// or invalid value falls back to its default. The keys, their defaults and
/// ranges are the same on every platform (see docs/development.md and
/// docs/settings.example.json).
public struct Settings: Equatable, Sendable {
    public var ankiConnectURL: String
    public var ankiConnectKey: String?
    public var cardQuery: String
    public var intervalMinutes: Int
    public var countdownSeconds: Int
    public var quietHoursEnabled: Bool
    public var quietHoursStart: ClockTime
    public var quietHoursEnd: ClockTime

    public init(
        ankiConnectURL: String, ankiConnectKey: String?, cardQuery: String, intervalMinutes: Int,
        countdownSeconds: Int, quietHoursEnabled: Bool, quietHoursStart: ClockTime,
        quietHoursEnd: ClockTime
    ) {
        self.ankiConnectURL = ankiConnectURL
        self.ankiConnectKey = ankiConnectKey
        self.cardQuery = cardQuery
        self.intervalMinutes = intervalMinutes
        self.countdownSeconds = countdownSeconds
        self.quietHoursEnabled = quietHoursEnabled
        self.quietHoursStart = quietHoursStart
        self.quietHoursEnd = quietHoursEnd
    }

    public static let defaults = Settings(
        ankiConnectURL: "http://127.0.0.1:8765",
        ankiConnectKey: nil,
        cardQuery: "is:due -is:suspended",
        intervalMinutes: 15,
        countdownSeconds: 30,
        quietHoursEnabled: false,
        quietHoursStart: ClockTime(hour: 22, minute: 0)!,
        quietHoursEnd: ClockTime(hour: 8, minute: 0)!)

    public static let intervalRange = 1...1440
    public static let countdownRange = 5...600

    /// The keys as they appear in the file.
    public enum Key: String, CaseIterable, Sendable {
        case ankiConnectURL = "ankiConnectUrl"
        case ankiConnectKey
        case cardQuery
        case intervalMinutes
        case countdownSeconds
        case quietHoursEnabled
        case quietHoursStart
        case quietHoursEnd
    }
}

/// What reading the settings file gave.
public struct SettingsLoad: Equatable, Sendable {
    public enum File: Equatable, Sendable {
        /// No file yet: the defaults apply, and the first save creates it.
        case missing
        case valid
        /// Not a JSON object: the defaults apply, and saving is refused so
        /// the file is never overwritten.
        case invalid(reason: String)
    }

    public var settings: Settings
    public var file: File
    /// One line per key whose value was ignored, for the log.
    public var problems: [String]

    public init(settings: Settings, file: File, problems: [String]) {
        self.settings = settings
        self.file = file
        self.problems = problems
    }

    /// The line the menu shows when the file cannot be used, nil otherwise.
    public var menuWarning: String? {
        if case .invalid = file { return "Settings file is invalid: using the defaults" }
        return nil
    }
}

public enum SettingsError: Error, Equatable {
    case fileIsInvalid(reason: String)
    case cannotWrite(String)
}

public enum SettingsStore {
    /// `SLIPCARD_SETTINGS` if set, otherwise
    /// `~/Library/Application Support/Slipcard/settings.json`.
    public static func defaultURL(environment: [String: String] = ProcessInfo.processInfo.environment)
        -> URL
    {
        if let path = environment["SLIPCARD_SETTINGS"], !path.isEmpty {
            return URL(fileURLWithPath: path)
        }
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return support.appendingPathComponent("Slipcard").appendingPathComponent("settings.json")
    }

    public static func load(from url: URL) -> SettingsLoad {
        guard let data = try? Data(contentsOf: url) else {
            if FileManager.default.fileExists(atPath: url.path) {
                return SettingsLoad(
                    settings: .defaults, file: .invalid(reason: "cannot be read"), problems: [])
            }
            return SettingsLoad(settings: .defaults, file: .missing, problems: [])
        }
        return parse(data)
    }

    public static func parse(_ data: Data) -> SettingsLoad {
        guard let object = try? JSONSerialization.jsonObject(with: data),
            let dict = object as? [String: Any]
        else {
            return SettingsLoad(
                settings: .defaults, file: .invalid(reason: "not a JSON object"), problems: [])
        }
        var s = Settings.defaults
        var problems: [String] = []
        func ignore(_ key: Settings.Key, expected: String, default value: CustomStringConvertible) {
            problems.append("\(key.rawValue): expected \(expected); using \(value)")
        }
        for key in Settings.Key.allCases {
            guard let raw = dict[key.rawValue] else { continue }
            switch key {
            case .ankiConnectURL:
                if let v = raw as? String, v.hasPrefix("http://") || v.hasPrefix("https://") {
                    s.ankiConnectURL = v
                } else {
                    ignore(key, expected: "an http:// or https:// address", default: s.ankiConnectURL)
                }
            case .ankiConnectKey:
                if raw is NSNull {
                    s.ankiConnectKey = nil
                } else if let v = raw as? String {
                    s.ankiConnectKey = v.isEmpty ? nil : v
                } else {
                    ignore(key, expected: "a string or null", default: "no key")
                }
            case .cardQuery:
                if let v = raw as? String, !v.trimmingCharacters(in: .whitespaces).isEmpty {
                    s.cardQuery = v
                } else {
                    ignore(key, expected: "a non-empty Anki search", default: s.cardQuery)
                }
            case .intervalMinutes:
                if let v = wholeNumber(raw), Settings.intervalRange.contains(v) {
                    s.intervalMinutes = v
                } else {
                    ignore(key, expected: "a whole number from 1 to 1440", default: s.intervalMinutes)
                }
            case .countdownSeconds:
                if let v = wholeNumber(raw), Settings.countdownRange.contains(v) {
                    s.countdownSeconds = v
                } else {
                    ignore(key, expected: "a whole number from 5 to 600", default: s.countdownSeconds)
                }
            case .quietHoursEnabled:
                if let v = boolean(raw) {
                    s.quietHoursEnabled = v
                } else {
                    ignore(key, expected: "true or false", default: s.quietHoursEnabled)
                }
            case .quietHoursStart:
                if let v = (raw as? String).flatMap(ClockTime.init) {
                    s.quietHoursStart = v
                } else {
                    ignore(key, expected: "a time \"HH:MM\"", default: s.quietHoursStart)
                }
            case .quietHoursEnd:
                if let v = (raw as? String).flatMap(ClockTime.init) {
                    s.quietHoursEnd = v
                } else {
                    ignore(key, expected: "a time \"HH:MM\"", default: s.quietHoursEnd)
                }
            }
        }
        return SettingsLoad(settings: s, file: .valid, problems: problems)
    }

    /// Writes `settings` to `url` atomically, keeping every key the file has
    /// that Slipcard does not know. Refuses to touch a file that is not a
    /// JSON object, so a hand edit with a typo is never lost.
    public static func save(_ settings: Settings, to url: URL) throws {
        var dict: [String: Any] = [:]
        if FileManager.default.fileExists(atPath: url.path) {
            guard let data = try? Data(contentsOf: url),
                let object = try? JSONSerialization.jsonObject(with: data),
                let existing = object as? [String: Any]
            else { throw SettingsError.fileIsInvalid(reason: "not a readable JSON object") }
            dict = existing
        }
        for (key, value) in fileValues(settings) { dict[key.rawValue] = value }
        do {
            let data = try JSONSerialization.data(
                withJSONObject: dict, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try (data + Data("\n".utf8)).write(to: url, options: .atomic)
        } catch {
            throw SettingsError.cannotWrite(error.localizedDescription)
        }
    }

    static func fileValues(_ s: Settings) -> [Settings.Key: Any] {
        [
            .ankiConnectURL: s.ankiConnectURL,
            .ankiConnectKey: s.ankiConnectKey.map { $0 as Any } ?? NSNull(),
            .cardQuery: s.cardQuery,
            .intervalMinutes: s.intervalMinutes,
            .countdownSeconds: s.countdownSeconds,
            .quietHoursEnabled: s.quietHoursEnabled,
            .quietHoursStart: s.quietHoursStart.description,
            .quietHoursEnd: s.quietHoursEnd.description,
        ]
    }

    /// JSONSerialization gives NSNumber for both numbers and booleans; a
    /// boolean or a fraction is not a whole number.
    private static func wholeNumber(_ raw: Any) -> Int? {
        guard let n = raw as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID() else { return nil }
        let d = n.doubleValue
        guard d.rounded() == d, abs(d) < 1e9 else { return nil }
        return Int(d)
    }

    private static func boolean(_ raw: Any) -> Bool? {
        guard let n = raw as? NSNumber, CFGetTypeID(n) == CFBooleanGetTypeID() else { return nil }
        return n.boolValue
    }
}
