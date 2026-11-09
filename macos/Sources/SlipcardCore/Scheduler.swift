import Foundation

/// Decides when a popup comes. Plain logic: it never reads the clock, the
/// caller passes the time to every call, and a function gives the UTC
/// offset at an instant so quiet hours work in local time. The app arms one
/// timer at `nextFire` and calls `tick` when it fires.
///
/// Rules:
/// - the first popup comes one interval after `start`;
/// - the next interval starts when the popup closes, so popups never stack;
/// - pause for an hour: no popup until the hour is over, then the first one
///   one interval after the pause ends; `resume` ends the pause at once;
/// - quiet hours: no popup inside the range (it may cross midnight); a popup
///   due inside it moves to the end of the range;
/// - next word now shows a popup at once, also during a pause or quiet
///   hours, and the interval restarts when that popup closes;
/// - after sleep or a long absence at most one popup comes, one interval
///   after wake: a tick more than `lateness` after the due time restarts the
///   wait instead of showing the missed popup.
public struct Scheduler: Sendable {
    public enum Phase: Equatable, Sendable {
        case waiting(due: Date)
        case showing
    }

    public typealias UTCOffset = @Sendable (Date) -> Int

    /// How late a tick may be and still show the popup it was armed for.
    public static let lateness: TimeInterval = 60
    public static let pauseLength: TimeInterval = 3600

    public private(set) var phase: Phase
    public private(set) var pausedUntil: Date?
    public private(set) var interval: TimeInterval
    public private(set) var quietHours: (start: ClockTime, end: ClockTime)?
    private let utcOffset: UTCOffset

    /// Starts waiting: the first popup is one interval after `now`.
    public init(
        settings: Settings, now: Date,
        utcOffset: @escaping UTCOffset = { TimeZone.current.secondsFromGMT(for: $0) }
    ) {
        interval = TimeInterval(settings.intervalMinutes * 60)
        quietHours = settings.quietHoursEnabled ? (settings.quietHoursStart, settings.quietHoursEnd) : nil
        self.utcOffset = utcOffset
        phase = .waiting(due: now.addingTimeInterval(interval))
    }

    /// New interval or quiet hours (from the tray or the settings file);
    /// the wait restarts from `now`.
    public mutating func apply(_ settings: Settings, now: Date) {
        interval = TimeInterval(settings.intervalMinutes * 60)
        quietHours = settings.quietHoursEnabled ? (settings.quietHoursStart, settings.quietHoursEnd) : nil
        if case .waiting = phase { phase = .waiting(due: now.addingTimeInterval(interval)) }
    }

    /// When the app should arm its timer next; nil while a popup is shown.
    public var nextFire: Date? {
        guard case .waiting(let due) = phase else { return nil }
        if let p = pausedUntil, due < p { return p }
        return isQuiet(due) ? quietEnd(after: due) : due
    }

    /// Called when the timer fires (or at any time). True: show a popup now.
    public mutating func tick(now: Date) -> Bool {
        guard case .waiting(let due) = phase else { return false }
        if let p = pausedUntil {
            if now < p { return false }
            pausedUntil = nil
        }
        guard now >= due else { return false }
        if now.timeIntervalSince(due) > Self.lateness {
            // Asleep or suspended through the due time: start over, no burst.
            phase = .waiting(due: now.addingTimeInterval(interval))
            return false
        }
        if isQuiet(now) {
            phase = .waiting(due: quietEnd(after: now))
            return false
        }
        phase = .showing
        return true
    }

    /// The popup closed (answered, closed or timed out).
    public mutating func popupClosed(now: Date) {
        phase = .waiting(due: now.addingTimeInterval(interval))
    }

    public mutating func pauseForAnHour(now: Date) {
        let end = now.addingTimeInterval(Self.pauseLength)
        pausedUntil = end
        if case .waiting = phase { phase = .waiting(due: end.addingTimeInterval(interval)) }
    }

    public mutating func resume(now: Date) {
        guard pausedUntil != nil else { return }
        pausedUntil = nil
        if case .waiting = phase { phase = .waiting(due: now.addingTimeInterval(interval)) }
    }

    public func isPaused(now: Date) -> Bool { pausedUntil.map { now < $0 } ?? false }

    /// "Next word now": true unless a popup is already on screen.
    public mutating func nextWordNow(now: Date) -> Bool {
        if case .showing = phase { return false }
        phase = .showing
        return true
    }

    /// The machine woke up: one interval from now before the next popup.
    public mutating func didWake(now: Date) {
        if case .waiting = phase { phase = .waiting(due: now.addingTimeInterval(interval)) }
    }

    // MARK: quiet hours

    /// Whether `date` falls inside quiet hours (start included, end not; a
    /// range with start equal to end is empty).
    public func isQuiet(_ date: Date) -> Bool {
        guard let (start, end) = quietHours else { return false }
        let s = start.hour * 60 + start.minute
        let e = end.hour * 60 + end.minute
        let m = minuteOfDay(date)
        if s == e { return false }
        return s < e ? (m >= s && m < e) : (m >= s || m < e)
    }

    /// The first instant after `date` at which quiet hours end.
    public func quietEnd(after date: Date) -> Date {
        guard let (_, end) = quietHours else { return date }
        let local = date.timeIntervalSince1970 + TimeInterval(utcOffset(date))
        let dayStart = (local / 86400).rounded(.down) * 86400
        var endLocal = dayStart + TimeInterval((end.hour * 60 + end.minute) * 60)
        if endLocal <= local { endLocal += 86400 }
        // The offset at the end itself, in case a DST change lies between.
        let guess = Date(timeIntervalSince1970: endLocal - TimeInterval(utcOffset(date)))
        return Date(timeIntervalSince1970: endLocal - TimeInterval(utcOffset(guess)))
    }

    private func minuteOfDay(_ date: Date) -> Int {
        let local = Int((date.timeIntervalSince1970 + TimeInterval(utcOffset(date))).rounded(.down))
        return ((local % 86400) + 86400) % 86400 / 60
    }
}
