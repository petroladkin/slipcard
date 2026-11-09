import AppKit
import SlipcardCore

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let showAfter: TimeInterval?
    private var statusItem: NSStatusItem?
    private var popup: PopupPanel?
    private(set) var settings = Settings.defaults
    private var client: AnkiConnectClient?
    private var flow = PopupFlow(scheduler: Scheduler(settings: .defaults, now: Date()))
    private var timer: Timer?
    private var wakeObserver: NSObjectProtocol?

    init(showAfter: TimeInterval?) {
        self.showAfter = showAfter
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let settingsURL = SettingsStore.defaultURL()
        let load = SettingsStore.load(from: settingsURL)
        settings = load.settings
        logSettings(load, at: settingsURL)
        client = AnkiConnectClient(settings: settings)
        if client == nil { NSLog("anki: the address is not a URL: %@", settings.ankiConnectURL) }
        flow = PopupFlow(scheduler: Scheduler(settings: settings, now: Date()))

        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.image = NSImage(
            systemSymbolName: "character.book.closed", accessibilityDescription: "Slipcard")
        let menu = NSMenu()
        if let text = load.menuWarning {
            let warning = NSMenuItem(title: text, action: nil, keyEquivalent: "")
            warning.isEnabled = false
            menu.addItem(warning)
            menu.addItem(.separator())
        }
        menu.addItem(withTitle: "Next word now", action: #selector(showNow), keyEquivalent: "")
        menu.addItem(withTitle: "Next word in 5 s", action: #selector(showSoon), keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit Slipcard", action: #selector(quit), keyEquivalent: "q")
        for menuItem in menu.items { menuItem.target = self }
        item.menu = menu
        statusItem = item

        // Sleep: one interval from wake before the next popup, no burst.
        wakeObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.flow.didWake(now: Date())
                self.arm()
            }
        }

        arm()
        NSLog("scheduler: every %d min; first popup at %@", settings.intervalMinutes, "\(flow.nextFire.map { "\($0)" } ?? "-")")
        if let showAfter { nextWordNow(after: showAfter) }
    }

    /// Goes to stderr and the system log (Console.app).
    private func logSettings(_ load: SettingsLoad, at url: URL) {
        switch load.file {
        case .missing: NSLog("settings: %@: no file yet, using the defaults", url.path)
        case .valid: NSLog("settings: %@: read", url.path)
        case .invalid(let reason):
            NSLog("settings: %@: invalid: %@; using the defaults", url.path, reason)
        }
        for problem in load.problems { NSLog("settings: %@", problem) }
    }

    @objc private func showNow() { nextWordNow(after: 0) }

    // Leaves time to switch to a full-screen app and start typing.
    @objc private func showSoon() { nextWordNow(after: 5) }

    @objc private func quit() { NSApp.terminate(nil) }

    private func nextWordNow(after seconds: TimeInterval) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.perform(self.flow.nextWordNow(now: Date()))
            }
        }
    }

    /// One timer, at the scheduler's next fire time; none while a popup is
    /// up or a card is being fetched.
    private func arm() {
        timer?.invalidate()
        timer = nil
        guard let fire = flow.nextFire else { return }
        let t = Timer(fire: fire, interval: 0, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.perform(self.flow.timerFired(now: Date()))
            }
        }
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }

    private func perform(_ step: PopupFlow.Step) {
        switch step {
        case .wait: arm()
        case .fetch: fetch()
        case .present(let card): present(card)
        }
    }

    private func fetch() {
        timer?.invalidate()
        let query = settings.cardQuery
        guard let client else {
            perform(flow.fetched(.failure(AnkiConnectError.badResponse("no AnkiConnect address")), now: Date()))
            return
        }
        Task { @MainActor in
            let result: Result<Card?, any Error>
            do {
                result = .success(try await client.nextDueCard(query: query))
            } catch {
                result = .failure(error)
            }
            let before = flow.anki
            let step = flow.fetched(result, now: Date())
            if flow.anki != before { NSLog("anki: %@", flow.anki.description) }
            if case .success(nil) = result { NSLog("popup: no card due for %@", query) }
            perform(step)
        }
    }

    private func present(_ card: Card) {
        guard let client else { return }
        popup?.close()
        let model = CardPopupModel(card: card, client: client)
        let panel = PopupPanel(model: model)
        panel.onClose = { [weak self] in
            guard let self else { return }
            self.popup = nil
            self.flow.popupClosed(now: Date())
            NSLog("popup: closed; next at %@", "\(self.flow.nextFire.map { "\($0)" } ?? "-")")
            self.arm()
        }
        panel.present()
        popup = panel
        NSLog("popup: card %lld shown", card.id)
    }
}
