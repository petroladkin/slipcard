import AppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let showAfter: TimeInterval?
    private var statusItem: NSStatusItem?
    private var popup: PopupPanel?

    init(showAfter: TimeInterval?) {
        self.showAfter = showAfter
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.image = NSImage(
            systemSymbolName: "character.book.closed", accessibilityDescription: "Slipcard")
        let menu = NSMenu()
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
