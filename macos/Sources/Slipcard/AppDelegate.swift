import AppKit
import SlipcardCore

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let showAfter: TimeInterval?
    private var statusItem: NSStatusItem?
    private var popup: PopupPanel?
    private(set) var settings = Settings.defaults
    private var settingsURL = SettingsStore.defaultURL()
    private var settingsWarning: String?
    private var saveProblem: String?
    private var client: AnkiConnectClient?
    private var flow = PopupFlow(scheduler: Scheduler(settings: .defaults, now: Date()))
    private var dueCount: Int?
    private var timer: Timer?
    private var wakeObserver: NSObjectProtocol?

    init(showAfter: TimeInterval?) {
        self.showAfter = showAfter
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let load = SettingsStore.load(from: settingsURL)
        settings = load.settings
        settingsWarning = load.menuWarning
        logSettings(load, at: settingsURL)
        client = AnkiConnectClient(settings: settings)
        if client == nil { NSLog("anki: the address is not a URL: %@", settings.ankiConnectURL) }
        flow = PopupFlow(scheduler: Scheduler(settings: settings, now: Date()))

        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.image = NSImage(
            systemSymbolName: "character.book.closed", accessibilityDescription: "Slipcard")
        let menu = NSMenu()
        menu.delegate = self
        menu.autoenablesItems = false
        item.menu = menu
        statusItem = item
        updateTray()
        refreshDueCount()

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

    // MARK: the tray

    /// The menu is rebuilt from the model whenever the state changes.
    private func updateTray() {
        guard let statusItem, let menu = statusItem.menu else { return }
        let state = flow.trayState(
            now: Date(), dueCount: dueCount, settings: settings, settingsWarning: settingsWarning,
            saveProblem: saveProblem)
        let model = TrayMenu(state)
        menu.removeAllItems()
        for item in model.items { menu.addItem(menuItem(item)) }
        statusItem.button?.appearsDisabled = model.iconDimmed
        statusItem.button?.toolTip = model.stateLines.joined(separator: "\n")
    }

    private func menuItem(_ item: TrayMenu.Item) -> NSMenuItem {
        switch item.kind {
        case .separator:
            return .separator()
        case .info:
            let m = NSMenuItem(title: item.title, action: nil, keyEquivalent: "")
            m.isEnabled = false
            return m
        case .action(let action), .toggle(let action, _):
            let m = NSMenuItem(
                title: item.title, action: #selector(trayItemChosen(_:)),
                keyEquivalent: action == .quit ? "q" : "")
            m.target = self
            m.representedObject = ActionBox(action)
            if case .toggle(_, let checked) = item.kind { m.state = checked ? .on : .off }
            return m
        case .submenu(let items):
            let m = NSMenuItem(title: item.title, action: nil, keyEquivalent: "")
            let sub = NSMenu(title: item.title)
            sub.autoenablesItems = false
            for child in items { sub.addItem(menuItem(child)) }
            m.submenu = sub
            return m
        }
    }

    func menuWillOpen(_ menu: NSMenu) {
        guard menu === statusItem?.menu else { return }
        updateTray()
        refreshDueCount()
    }

    @objc private func trayItemChosen(_ sender: NSMenuItem) {
        guard let action = (sender.representedObject as? ActionBox)?.action else { return }
        perform(action)
    }

    func perform(_ action: TrayAction) {
        switch action {
        case .nextWordNow: nextWordNow(after: 0)
        case .nextWordSoon: nextWordNow(after: 5)
        case .pause:
            flow.pauseForAnHour(now: Date())
            NSLog("tray: paused for an hour")
            arm()
        case .resume:
            flow.resume(now: Date())
            NSLog("tray: resumed")
            arm()
        case .toggleQuietHours, .setInterval:
            guard let r = TraySettingsChange.perform(action, on: settings, file: settingsURL) else { break }
            settings = r.settings
            saveProblem = r.saveProblem
            flow.apply(settings, now: Date())
            NSLog(
                "tray: interval %d min, quiet hours %@%@", settings.intervalMinutes,
                settings.quietHoursEnabled ? "on" : "off", r.saveProblem.map { "; \($0)" } ?? "; saved")
            arm()
        case .openSettings: openSettingsFile()
        case .quit: NSApp.terminate(nil)
        }
        updateTray()
    }

    /// Opens the file in the default editor; a missing file is created first
    /// with the settings in effect.
    private func openSettingsFile() {
        if !FileManager.default.fileExists(atPath: settingsURL.path) {
            do {
                try SettingsStore.save(settings, to: settingsURL)
            } catch {
                NSLog("settings: cannot create %@: %@", settingsURL.path, "\(error)")
            }
        }
        NSWorkspace.shared.open(settingsURL)
    }

    /// The due count for the tray (one findCards); it also tells whether
    /// Anki answers.
    private func refreshDueCount() {
        guard let client else { return }
        let query = settings.cardQuery
        Task { @MainActor in
            do {
                dueCount = try await client.dueCount(query: query)
                flow.connectionChecked(nil)
            } catch {
                let before = flow.anki
                flow.connectionChecked(error)
                if flow.anki != before { NSLog("anki: %@", flow.anki.description) }
            }
            updateTray()
        }
    }

    // MARK: popups

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
                let due = try await client.dueCards(query: query)
                dueCount = due.count
                result = .success(due.next)
            } catch {
                result = .failure(error)
            }
            let before = flow.anki
            let step = flow.fetched(result, now: Date())
            if flow.anki != before { NSLog("anki: %@", flow.anki.description) }
            if case .success(nil) = result { NSLog("popup: no card due for %@", query) }
            updateTray()
            perform(step)
        }
    }

    private func present(_ card: Card) {
        guard let client else { return }
        popup?.close()
        let model = CardPopupModel(
            card: card, client: client, countdownSeconds: settings.countdownSeconds, shownAt: Date())
        let panel = PopupPanel(model: model)
        panel.onClose = { [weak self] in
            guard let self else { return }
            self.popup = nil
            if model.countdown.isExpired(at: Date()) {
                NSLog("popup: card %lld closed by the countdown, not answered", card.id)
            }
            self.flow.popupClosed(now: Date())
            NSLog("popup: closed; next at %@", "\(self.flow.nextFire.map { "\($0)" } ?? "-")")
            self.arm()
            self.refreshDueCount()
        }
        panel.present()
        popup = panel
        NSLog("popup: card %lld shown", card.id)
    }
}

/// A TrayAction on an NSMenuItem.
private final class ActionBox: NSObject {
    let action: TrayAction
    init(_ action: TrayAction) { self.action = action }
}
