import AppKit

/// The icon in the menu bar. Its menu shows the app state and has "Quit".
final class StatusMenu: NSObject, NSMenuDelegate {
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    private let status: () -> [String]
    private let quit: () -> Void

    init(status: @escaping () -> [String], quit: @escaping () -> Void) {
        self.status = status
        self.quit = quit
        super.init()
        item.button?.image = NSImage(systemSymbolName: "gamecontroller", accessibilityDescription: "Kuterlapa")
        let menu = NSMenu()
        menu.delegate = self
        item.menu = menu
    }

    /// Builds the menu each time it opens, so the state is current.
    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        for line in status() {
            let entry = NSMenuItem(title: line, action: nil, keyEquivalent: "")
            entry.isEnabled = false
            menu.addItem(entry)
        }
        menu.addItem(.separator())
        let quitItem = NSMenuItem(title: "Zakończ Kuterlapę", action: #selector(quitClicked), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)
    }

    @objc private func quitClicked() {
        quit()
    }
}
