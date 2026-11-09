import Foundation

/// What the app does when the scheduler says "now": fetch the next due card,
/// then show it, or log why not and wait for the next interval. Plain logic:
/// the app performs each step (arm the timer, fetch off the main thread,
/// present the popup) and reports back.
public struct PopupFlow: Sendable {
    public enum Step: Equatable, Sendable {
        /// Nothing to show now: arm the timer at `nextFire`.
        case wait
        /// Ask AnkiConnect for the next due card, then call `fetched`.
        case fetch
        /// Show this card; call `popupClosed` when the popup goes away.
        case present(Card)
    }

    public private(set) var scheduler: Scheduler
    /// The connection state as of the last fetch, for the log and the tray.
    public private(set) var anki: AnkiState = .unknown

    public init(scheduler: Scheduler) {
        self.scheduler = scheduler
    }

    /// When to arm the timer; nil while a popup is shown or a card is fetched.
    public var nextFire: Date? { scheduler.nextFire }

    /// The timer fired.
    public mutating func timerFired(now: Date) -> Step {
        scheduler.tick(now: now) ? .fetch : .wait
    }

    /// "Next word now" from the menu: at once, unless a popup is up.
    public mutating func nextWordNow(now: Date) -> Step {
        scheduler.nextWordNow(now: now) ? .fetch : .wait
    }

    /// The fetch finished: a card is shown; no card due, or an error, means
    /// no popup and one more interval to wait.
    public mutating func fetched(_ result: Result<Card?, any Error>, now: Date) -> Step {
        switch result {
        case .success(let card?):
            anki = .connected
            return .present(card)
        case .success(nil):
            anki = .connected
        case .failure(let error):
            anki = AnkiState(error)
        }
        scheduler.popupClosed(now: now)
        return .wait
    }

    /// The popup went away (graded, closed, or timed out).
    public mutating func popupClosed(now: Date) {
        scheduler.popupClosed(now: now)
    }

    /// The machine woke from sleep.
    public mutating func didWake(now: Date) {
        scheduler.didWake(now: now)
    }

    // MARK: the tray

    public mutating func pauseForAnHour(now: Date) { scheduler.pauseForAnHour(now: now) }

    public mutating func resume(now: Date) { scheduler.resume(now: now) }

    /// New interval or quiet hours from the tray; the wait restarts.
    public mutating func apply(_ settings: Settings, now: Date) { scheduler.apply(settings, now: now) }

    /// A check outside a popup fetch (the tray's due count) learnt the state.
    public mutating func connectionChecked(_ error: (any Error)?) {
        anki = error.map(AnkiState.init) ?? .connected
    }

    /// What the tray shows at `now`.
    public func trayState(
        now: Date, dueCount: Int?, settings: Settings, settingsWarning: String?, saveProblem: String?
    ) -> TrayState {
        TrayState(
            anki: anki, dueCount: dueCount,
            pausedUntil: scheduler.isPaused(now: now) ? scheduler.pausedUntil : nil,
            quietUntil: scheduler.isQuiet(now) ? scheduler.quietEnd(after: now) : nil,
            settingsWarning: settingsWarning, saveProblem: saveProblem, settings: settings)
    }
}

/// The card popup's state, apart from the window: what to show and what the
/// buttons do. The panel renders it and calls it; tests call it the same way.
@MainActor
public final class CardPopupModel {
    public enum Stage: Equatable, Sendable {
        case front
        case back
        case sending(Ease)
        /// The grade was not recorded; the reason is shown, and grading again
        /// is allowed.
        case failed(String)
        case closed
    }

    public typealias Answer = @Sendable (Int64, Ease) async throws -> Void

    public let card: Card
    public let front: String
    public let back: String
    public private(set) var stage: Stage = .front {
        didSet { onChange?() }
    }
    /// The Close button's countdown; it stops for good on a reaction.
    public private(set) var countdown: Countdown
    /// Called after every stage change.
    public var onChange: (() -> Void)?
    /// Called once, when the popup should go away.
    public var onClose: (() -> Void)?

    private let answer: Answer

    /// `countdownSeconds` comes from the settings; `shownAt` is when the
    /// popup appears, where the countdown starts.
    public init(
        card: Card, countdownSeconds: Int = Settings.defaults.countdownSeconds,
        shownAt: Date = Date(), answer: @escaping Answer
    ) {
        self.card = card
        front = CardText.plain(card.questionHTML)
        back = CardText.plain(card.answerHTML)
        countdown = Countdown(seconds: countdownSeconds, started: shownAt)
        self.answer = answer
    }

    public convenience init(
        card: Card, client: AnkiConnectClient,
        countdownSeconds: Int = Settings.defaults.countdownSeconds, shownAt: Date = Date()
    ) {
        self.init(card: card, countdownSeconds: countdownSeconds, shownAt: shownAt) { id, ease in
            try await client.answer(cardID: id, ease: ease)
        }
    }

    /// The Close button's title at `now`: "Close (N)", or "Close" once the
    /// countdown stopped.
    public func closeLabel(at now: Date) -> String { countdown.label(at: now) }

    /// The countdown timer ticked: at zero the popup closes, without an
    /// answer, the same way the Close button closes it.
    public func tick(now: Date) {
        guard stage != .closed, countdown.isExpired(at: now) else { return }
        close()
    }

    /// The pointer moved over the popup: the user reacted, so the countdown
    /// stops for good. A pointer that only rests where the popup appeared
    /// sends no movement and does not count.
    public func pointerMoved() {
        guard !countdown.isStopped else { return }
        countdown.stop()
        onChange?()
    }

    public var showsAnswer: Bool { stage != .front }

    /// Again / Hard / Good / Easy are on screen (and enabled unless sending).
    public var showsGrades: Bool {
        switch stage {
        case .back, .sending, .failed: return true
        case .front, .closed: return false
        }
    }

    public var isSending: Bool {
        if case .sending = stage { return true }
        return false
    }

    public var failure: String? {
        if case .failed(let reason) = stage { return reason }
        return nil
    }

    /// Show; it stops the countdown for good, like a pointer over the popup.
    public func reveal() {
        countdown.stop()
        if stage == .front { stage = .back }
    }

    /// A grade button: records the review, then closes; on failure the card
    /// stays unanswered and the popup says why.
    public func grade(_ ease: Ease) async {
        guard showsGrades, !isSending else { return }
        stage = .sending(ease)
        do {
            try await answer(card.id, ease)
            close()
        } catch {
            stage = .failed("Not saved: \(AnkiState(error)). The card was not answered.")
        }
    }

    /// Close: the card is not answered.
    public func close() {
        guard stage != .closed else { return }
        stage = .closed
        let done = onClose
        onClose = nil
        done?()
    }
}
