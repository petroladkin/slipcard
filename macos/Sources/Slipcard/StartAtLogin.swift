import AppKit
import ServiceManagement
import SlipcardCore

/// "Start at login" through SMAppService, for the installed app bundle only:
/// a copy run with `swift run` has no bundle to register.
@MainActor
enum StartAtLogin {
    static var isBundled: Bool { Bundle.main.bundleURL.pathExtension == "app" }

    /// What the tray shows; read each time the menu opens.
    static func state() -> LoginItem {
        guard isBundled else { return .unavailable }
        return LoginItem(serviceStatus: SMAppService.mainApp.status.rawValue)
    }

    /// Turns it on or off. Returns the problem for the tray, nil when done.
    /// When the system wants the user's approval, its Login Items settings
    /// open instead.
    static func toggle() -> String? {
        guard isBundled else { return "Start at login works only from the installed app" }
        let service = SMAppService.mainApp
        do {
            switch service.status {
            case .enabled:
                try service.unregister()
            case .requiresApproval:
                SMAppService.openSystemSettingsLoginItems()
            default:
                try service.register()
                if service.status == .requiresApproval { SMAppService.openSystemSettingsLoginItems() }
            }
            NSLog("tray: start at login %@", "\(state())")
            return nil
        } catch {
            NSLog("tray: start at login failed: %@", "\(error)")
            return "Start at login: \(error.localizedDescription)"
        }
    }
}
