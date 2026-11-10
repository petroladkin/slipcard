import Foundation

/// What the tray menu can do.
public enum TrayAction: Equatable, Sendable {
    case nextWordNow
    /// Leaves time to switch to a full-screen app and start typing.
    case nextWordSoon
    case pause
    case resume
    case toggleQuietHours
    case setInterval(minutes: Int)
    case openSettings
    /// Starts Slipcard when the user logs in, or stops doing so.
    case toggleStartAtLogin
    case quit

    /// The actions by a name for the command line (`--tray-action
    /// interval:30`, `quiet-hours`, `start-at-login`).
    public init?(commandLine text: String) {
        if text == "quiet-hours" { self = .toggleQuietHours; return }
        if text == "start-at-login" { self = .toggleStartAtLogin; return }
        if text.hasPrefix("interval:"), let n = Int(text.dropFirst("interval:".count)) {
            self = .setInterval(minutes: n)
            return
        }
        return nil
    }

    /// The settings after this action, or nil when it does not change them
    /// (or would make them invalid).
    public func applied(to settings: Settings) -> Settings? {
        var s = settings
        switch self {
        case .toggleQuietHours:
            s.quietHoursEnabled.toggle()
        case .setInterval(let minutes):
            guard Settings.intervalRange.contains(minutes), minutes != s.intervalMinutes else { return nil }
            s.intervalMinutes = minutes
        default:
            return nil
        }
        return s
    }
}

/// Whether Slipcard starts at login, as the system reports it.
public enum LoginItem: Equatable, Sendable {
    /// Not possible from this copy (on macOS: not running as the installed
    /// app bundle).
    case unavailable
    case off
    case on
    /// Registered, but the user has to allow it in System Settings.
    case needsApproval

    /// From `SMAppService.Status.rawValue`: 0 not registered, 1 enabled,
    /// 2 requires approval, 3 not found.
    public init(serviceStatus raw: Int) {
        switch raw {
        case 0: self = .off
        case 1: self = .on
        case 2: self = .needsApproval
        default: self = .unavailable
        }
    }
}

/// What the tray shows; the app fills it from the scheduler, the last fetch
/// and the settings.
public struct TrayState: Equatable, Sendable {
    public var anki: AnkiState
    /// Cards matching the search, as of the last check; nil when unknown.
    public var dueCount: Int?
    /// Set while a pause runs.
    public var pausedUntil: Date?
    /// Set while quiet hours run: when they end.
    public var quietUntil: Date?
    /// The settings file cannot be used (its menu warning).
    public var settingsWarning: String?
    /// The last change from the tray could not be written.
    public var saveProblem: String?
    public var settings: Settings
    public var startAtLogin: LoginItem
    /// The last Start at login change failed.
    public var loginProblem: String?

    public init(
        anki: AnkiState = .unknown, dueCount: Int? = nil, pausedUntil: Date? = nil,
        quietUntil: Date? = nil, settingsWarning: String? = nil, saveProblem: String? = nil,
        settings: Settings = .defaults, startAtLogin: LoginItem = .unavailable, loginProblem: String? = nil
    ) {
        self.startAtLogin = startAtLogin
        self.loginProblem = loginProblem
        self.anki = anki
        self.dueCount = dueCount
        self.pausedUntil = pausedUntil
        self.quietUntil = quietUntil
        self.settingsWarning = settingsWarning
        self.saveProblem = saveProblem
        self.settings = settings
    }
}

/// The tray menu as plain data: the macOS menu and the Linux tray are built
/// from it, and the tests read it. Same rules as tray_menu.rs.
public struct TrayMenu: Equatable, Sendable {
    public struct Item: Equatable, Sendable {
        public enum Kind: Equatable, Sendable {
            /// A line of state, not clickable.
            case info
            case action(TrayAction)
            /// A switch; `checked` shows its state.
            case toggle(TrayAction, checked: Bool)
            case submenu([Item])
            case separator
        }

        public let title: String
        public let kind: Kind

        public static let separator = Item(title: "", kind: .separator)
    }

    public static let intervalChoices = [5, 10, 15, 30, 60]

    public let items: [Item]
    /// The icon greys out when Anki is not reachable.
    public let iconDimmed: Bool

    /// The state lines alone, also used as the tooltip.
    public let stateLines: [String]

    public init(_ state: TrayState, utcOffset: (Date) -> Int = { TimeZone.current.secondsFromGMT(for: $0) }) {
        func clock(_ date: Date) -> String {
            let local = Int((date.timeIntervalSince1970 + TimeInterval(utcOffset(date))).rounded(.down))
            let minute = ((local % 86400) + 86400) % 86400 / 60
            return String(format: "%02d:%02d", minute / 60, minute % 60)
        }
        var lines: [String] = []
        if let warning = state.settingsWarning { lines.append(warning) }
        if let problem = state.saveProblem { lines.append(problem) }
        if let problem = state.loginProblem { lines.append(problem) }
        lines.append(state.anki.description)
        if state.anki == .connected, let n = state.dueCount {
            lines.append(n == 0 ? "No cards due" : n == 1 ? "1 card due" : "\(n) cards due")
        }
        if let until = state.pausedUntil { lines.append("Paused until \(clock(until))") }
        if let until = state.quietUntil { lines.append("Quiet hours until \(clock(until))") }
        stateLines = lines

        let s = state.settings
        var minutes = Self.intervalChoices
        if !minutes.contains(s.intervalMinutes) { minutes = (minutes + [s.intervalMinutes]).sorted() }
        let interval = minutes.map {
            Item(title: "\($0) min", kind: .toggle(.setInterval(minutes: $0), checked: $0 == s.intervalMinutes))
        }
        items = lines.map { Item(title: $0, kind: .info) } + [
            .separator,
            Item(title: "Next word now", kind: .action(.nextWordNow)),
            Item(title: "Next word in 5 s", kind: .action(.nextWordSoon)),
            state.pausedUntil == nil
                ? Item(title: "Pause for an hour", kind: .action(.pause))
                : Item(title: "Resume", kind: .action(.resume)),
            Item(
                title: "Quiet hours \(s.quietHoursStart)–\(s.quietHoursEnd)",
                kind: .toggle(.toggleQuietHours, checked: s.quietHoursEnabled)),
            Item(title: "Interval", kind: .submenu(interval)),
            .separator,
            Item(title: "Open settings file", kind: .action(.openSettings)),
            Self.startAtLoginItem(state.startAtLogin),
            Item(title: "Quit Slipcard", kind: .action(.quit)),
        ]
        if case .failed = state.anki { iconDimmed = true } else { iconDimmed = false }
    }

    static func startAtLoginItem(_ state: LoginItem) -> Item {
        switch state {
        case .unavailable:
            return Item(title: "Start at login: only from the installed app", kind: .info)
        case .off:
            return Item(title: "Start at login", kind: .toggle(.toggleStartAtLogin, checked: false))
        case .on:
            return Item(title: "Start at login", kind: .toggle(.toggleStartAtLogin, checked: true))
        case .needsApproval:
            return Item(
                title: "Start at login: allow it in System Settings",
                kind: .toggle(.toggleStartAtLogin, checked: true))
        }
    }
}

/// A settings change from the tray: applied, then written to the file. The
/// menu and `Slipcard --tray-action` both go through here.
public enum TraySettingsChange {
    public struct Result: Equatable, Sendable {
        /// The settings now in effect (in memory even if not written).
        public let settings: Settings
        /// Why the file was not written, for the tray; nil when it was.
        public let saveProblem: String?
    }

    /// Nil when `action` does not change the settings.
    public static func perform(_ action: TrayAction, on settings: Settings, file url: URL) -> Result? {
        guard let changed = action.applied(to: settings) else { return nil }
        do {
            try SettingsStore.save(changed, to: url)
            return Result(settings: changed, saveProblem: nil)
        } catch SettingsError.fileIsInvalid {
            return Result(settings: changed, saveProblem: "Not saved: the settings file is invalid")
        } catch {
            return Result(settings: changed, saveProblem: "Not saved: the settings file cannot be written")
        }
    }
}
