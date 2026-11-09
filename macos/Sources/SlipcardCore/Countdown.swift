import Foundation

/// The Close button's countdown: "Close (N)" ticking down from the configured
/// seconds; at zero the popup closes, which is never an answer. Once the user
/// reacts (the pointer moves over the popup, or Show) it stops for good and
/// the button reads "Close". Plain logic: the time is passed in.
public struct Countdown: Equatable, Sendable {
    public let seconds: Int
    public let started: Date
    public private(set) var isStopped = false

    public init(seconds: Int, started: Date) {
        self.seconds = max(0, seconds)
        self.started = started
    }

    /// Whole seconds left at `now`, rounded up (the full count at the start,
    /// 1 in the last second, 0 at the end); nil once stopped.
    public func remaining(at now: Date) -> Int? {
        guard !isStopped else { return nil }
        let left = Double(seconds) - now.timeIntervalSince(started)
        return max(0, Int(left.rounded(.up)))
    }

    /// The Close button's title at `now`.
    public func label(at now: Date) -> String {
        guard let left = remaining(at: now) else { return "Close" }
        return "Close (\(left))"
    }

    /// The popup should close now: the count reached zero and nobody reacted.
    public func isExpired(at now: Date) -> Bool {
        remaining(at: now) == 0
    }

    /// Final: a stopped countdown never runs again.
    public mutating func stop() {
        isStopped = true
    }
}
