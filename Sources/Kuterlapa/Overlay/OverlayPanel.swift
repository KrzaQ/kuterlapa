import AppKit

/// The overlay: a small panel in the top right corner of the screen.
///
/// It never takes the keyboard focus (a non-activating panel), so keys
/// from the controller still reach the application under it. Mouse clicks
/// pass through it. It shows on every desktop and over full-screen apps.
final class OverlayPanel {
    private let panel: NSPanel
    private let titleLabel = OverlayPanel.label(size: 12, weight: .semibold, color: .secondaryLabelColor)
    private let statusLabel = OverlayPanel.label(size: 12, weight: .regular, color: .systemOrange)
    private let textLabel = OverlayPanel.label(size: 15, weight: .regular, color: .labelColor)
    private let hintLabel = OverlayPanel.label(size: 11, weight: .regular, color: .tertiaryLabelColor)
    private let width: CGFloat = 440
    private let margin: CGFloat = 12

    init() {
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 440, height: 120),
                        styleMask: [.nonactivatingPanel, .borderless],
                        backing: .buffered, defer: true)
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.ignoresMouseEvents = true
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false

        let background = NSVisualEffectView()
        background.material = .hudWindow
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 12
        background.layer?.masksToBounds = true

        let stack = NSStackView(views: [titleLabel, statusLabel, textLabel, hintLabel])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 6
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 14, bottom: 12, right: 14)
        stack.translatesAutoresizingMaskIntoConstraints = false
        background.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: background.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: background.trailingAnchor),
            stack.topAnchor.constraint(equalTo: background.topAnchor),
            stack.bottomAnchor.constraint(equalTo: background.bottomAnchor),
            stack.widthAnchor.constraint(equalToConstant: width),
        ])
        for label in [titleLabel, statusLabel, textLabel, hintLabel] {
            label.preferredMaxLayoutWidth = width - 28
        }
        panel.contentView = background
    }

    /// Shows or hides the panel. `state` comes from the core as JSON:
    /// {"visible": true, "title": "...", "status": "...", "text": "...", "hint": "..."}
    func update(_ state: [String: Any]) {
        guard state["visible"] as? Bool == true else {
            panel.orderOut(nil)
            return
        }
        set(titleLabel, state["title"])
        set(statusLabel, state["status"])
        set(textLabel, state["text"])
        set(hintLabel, state["hint"])
        panel.contentView?.layoutSubtreeIfNeeded()
        let size = panel.contentView?.fittingSize ?? NSSize(width: width, height: 120)
        let screen = NSScreen.main?.visibleFrame ?? .zero
        let height = min(size.height, screen.height - 2 * margin)
        panel.setFrame(NSRect(x: screen.maxX - size.width - margin, y: screen.maxY - height - margin,
                              width: size.width, height: height), display: true)
        panel.orderFrontRegardless()
    }

    private func set(_ label: NSTextField, _ value: Any?) {
        let text = value as? String ?? ""
        label.stringValue = text
        label.isHidden = text.isEmpty
    }

    private static func label(size: CGFloat, weight: NSFont.Weight, color: NSColor) -> NSTextField {
        let label = NSTextField(wrappingLabelWithString: "")
        label.font = .systemFont(ofSize: size, weight: weight)
        label.textColor = color
        label.isSelectable = false
        return label
    }
}
