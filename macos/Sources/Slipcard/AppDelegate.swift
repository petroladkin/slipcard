import AppKit
import SlipcardCore

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let showAfter: TimeInterval?
    private var statusItem: NSStatusItem?
    private var popup: PopupPanel?
    /// Read at launch; the scheduler and the tray (next steps) use it.
    private(set) var settings = Settings.defaults

    init(showAfter: TimeInterval?) {
        self.showAfter = showAfter
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let settingsURL = SettingsStore.defaultURL()
        let load = SettingsStore.load(from: settingsURL)
        settings = load.settings
        logSettings(load, at: settingsURL)

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

        if let showAfter {
            schedule(after: showAfter)
        }
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

    @objc private func showNow() { show() }

    // Leaves time to switch to a full-screen app and start typing.
    @objc private func showSoon() { schedule(after: 5) }

    @objc private func quit() { NSApp.terminate(nil) }

    private func schedule(after seconds: TimeInterval) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in
            self?.show()
        }
    }

    private func show() {
        popup?.close()
        let panel = PopupPanel(
            word: "serendipity",
            answer: "a happy accident\n“Finding this café was pure serendipity.”")
        panel.onClose = { [weak self] in self?.popup = nil }
        panel.present()
        popup = panel
    }
}
