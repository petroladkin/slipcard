import AppKit

// A menu bar app whose popup shows the next due Anki card over
// full-screen apps without taking keyboard focus.
//
//   swift run Slipcard                    menu bar item only
//   swift run Slipcard --show-after 5     also shows a popup after 5 s
//   swift run Slipcard --next-card        prints the card a popup would show now, and exits
//   swift run Slipcard --tray-action interval:30   runs a settings action of the tray, and exits

if let i = CommandLine.arguments.firstIndex(of: "--tray-action"), i + 1 < CommandLine.arguments.count {
    exit(MainActor.assumeIsolated { trayActionCheck(CommandLine.arguments[i + 1]) })
}

if CommandLine.arguments.contains("--next-card") {
    Task { exit(await nextCardCheck()) }
    dispatchMain()
}

let app = NSApplication.shared
let delegate = AppDelegate(showAfter: CommandLine.showAfter)
app.delegate = delegate
// An accessory app has no Dock icon and is never activated by showing a panel.
app.setActivationPolicy(.accessory)
app.run()

extension CommandLine {
    static var showAfter: TimeInterval? {
        guard let i = arguments.firstIndex(of: "--show-after"), i + 1 < arguments.count else {
            return nil
        }
        return TimeInterval(arguments[i + 1])
    }
}
