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
        item.button?.image = Self.icon()
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
        let login = NSMenuItem(title: "Uruchamiaj przy logowaniu", action: #selector(loginClicked), keyEquivalent: "")
        login.target = self
        login.state = LoginItem.enabled ? .on : .off
        menu.addItem(login)
        menu.addItem(.separator())
        let quitItem = NSMenuItem(title: "Zakończ Kuterlapę", action: #selector(quitClicked), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)
    }

    @objc private func loginClicked() {
        NSLog("%@", LoginItem.set(!LoginItem.enabled))
    }

    @objc private func quitClicked() {
        quit()
    }

    /// The app icon, a paw with a sound wave, simplified for the menu bar.
    /// A template image: macOS colors it to match the menu bar.
    private static func icon() -> NSImage {
        let image = NSImage(size: NSSize(width: 24, height: 18), flipped: true) { _ in
            NSColor.black.set()
            NSBezierPath(ovalIn: NSRect(x: 4, y: 8.8, width: 10, height: 8.4)).fill()
            for (x, y) in [(4.0, 7.0), (7.5, 4.2), (11.5, 4.2), (15.0, 7.0)] {
                NSBezierPath(ovalIn: NSRect(x: x - 1.9, y: y - 2.3, width: 3.8, height: 4.6)).fill()
            }
            for (x, top, bottom, bulge) in [(18.0, 10.0, 16.0, 2.5), (21.0, 8.0, 18.0, 3.5)] {
                let arc = NSBezierPath()
                arc.move(to: NSPoint(x: x, y: top))
                arc.curve(to: NSPoint(x: x, y: bottom),
                          controlPoint1: NSPoint(x: x + bulge, y: top + (bottom - top) * 0.3),
                          controlPoint2: NSPoint(x: x + bulge, y: top + (bottom - top) * 0.7))
                arc.lineWidth = 1.6
                arc.lineCapStyle = .round
                arc.stroke()
            }
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "Kuterlapa"
        return image
    }
}
