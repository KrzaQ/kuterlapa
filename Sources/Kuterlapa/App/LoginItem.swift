import Foundation
import ServiceManagement

/// Starts Kuterlapa when the user logs in, as a login item (System Settings,
/// General, Login Items). On by default; the menu bar menu turns it off,
/// and the choice is kept.
enum LoginItem {
    private static let key = "startAtLogin"

    /// The user's choice; true until they turn it off.
    static var wanted: Bool {
        UserDefaults.standard.object(forKey: key) as? Bool ?? true
    }

    static var enabled: Bool {
        SMAppService.mainApp.status == .enabled
    }

    /// At start: make the login item match the user's choice.
    static func apply() -> String {
        do {
            if wanted && !enabled {
                try SMAppService.mainApp.register()
            } else if !wanted && enabled {
                try SMAppService.mainApp.unregister()
            }
        } catch {
            return "login item: \(error.localizedDescription)"
        }
        switch SMAppService.mainApp.status {
        case .enabled: return "login item: on"
        case .requiresApproval: return "login item: waiting for approval in System Settings"
        case .notRegistered: return "login item: off"
        default: return "login item: not found"
        }
    }

    static func set(_ on: Bool) -> String {
        UserDefaults.standard.set(on, forKey: key)
        return apply()
    }
}
