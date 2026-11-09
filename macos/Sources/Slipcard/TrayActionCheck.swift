import Foundation
import SlipcardCore

/// `Slipcard --tray-action interval:30` (or `quiet-hours`): runs a settings
/// action of the tray the way the menu does (TraySettingsChange) and exits;
/// no menu bar item, no popup. Exit status: 0 saved, 2 applied but not
/// saved, 1 not a settings action or nothing changed.
@MainActor
func trayActionCheck(_ name: String) -> Int32 {
    let url = SettingsStore.defaultURL()
    let load = SettingsStore.load(from: url)
    print("settings: \(url.path) (\(load.file))")
    guard let action = TrayAction(commandLine: name) else {
        print("tray: not a settings action: \(name) (interval:N or quiet-hours)")
        return 1
    }
    guard let r = TraySettingsChange.perform(action, on: load.settings, file: url) else {
        print("tray: \(name) changes nothing")
        return 1
    }
    print("tray: interval \(r.settings.intervalMinutes) min, quiet hours \(r.settings.quietHoursEnabled ? "on" : "off")")
    print(r.saveProblem ?? "tray: saved")
    return r.saveProblem == nil ? 0 : 2
}
