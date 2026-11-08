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
    private let headingLabel = OverlayPanel.label(size: 22, weight: .bold, color: .labelColor)
    private let textLabel = OverlayPanel.label(size: 15, weight: .regular, color: .labelColor)
    private let hintLabel = OverlayPanel.label(size: 11, weight: .regular, color: .tertiaryLabelColor)
    private let detailLabel = OverlayPanel.label(size: 12, weight: .regular, color: .secondaryLabelColor)
    private let buttonRow = NSStackView()
    private var background: NSVisualEffectView!
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

        background = NSVisualEffectView()
        background.material = .hudWindow
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 12
        background.layer?.masksToBounds = true

        buttonRow.orientation = .horizontal
        buttonRow.distribution = .fillEqually
        buttonRow.spacing = 10
        buttonRow.isHidden = true

        let stack = NSStackView(views: [titleLabel, statusLabel, headingLabel, textLabel, detailLabel, buttonRow, hintLabel])
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
            buttonRow.widthAnchor.constraint(equalToConstant: width - 28),
        ])
        for label in [titleLabel, statusLabel, headingLabel, textLabel, detailLabel, hintLabel] {
            label.preferredMaxLayoutWidth = width - 28
        }
        panel.contentView = background
    }

    /// Shows or hides the panel. `state` comes from the core as JSON:
    /// {"visible": true, "title": "...", "status": "...", "text": "...", "hint": "..."}
    /// Optional: "heading" (large, above the text) and "detail" (small,
    /// below it).
    ///
    /// When the user has to decide, the core adds large buttons and asks for
    /// attention, which plays a sound and draws a frame around the panel:
    /// "buttons": [{"key": "A", "label": "Tak", "tone": "yes"},
    ///             {"key": "B", "label": "Nie", "tone": "no"}],
    /// "attention": true
    func update(_ state: [String: Any]) {
        guard state["visible"] as? Bool == true else {
            panel.orderOut(nil)
            return
        }
        set(titleLabel, state["title"])
        set(statusLabel, state["status"])
        set(headingLabel, state["heading"])
        set(textLabel, state["text"])
        set(detailLabel, state["detail"])
        set(hintLabel, state["hint"])
        setButtons(state["buttons"] as? [[String: Any]] ?? [])
        let attention = state["attention"] as? Bool == true
        background.layer?.borderWidth = attention ? 2 : 0
        background.layer?.borderColor = NSColor.systemOrange.withAlphaComponent(0.8).cgColor
        if attention && !wasAsking {
            NSSound(named: "Glass")?.play()
        }
        wasAsking = attention
        panel.contentView?.layoutSubtreeIfNeeded()
        let size = panel.contentView?.fittingSize ?? NSSize(width: width, height: 120)
        let screen = NSScreen.main?.visibleFrame ?? .zero
        let height = min(size.height, screen.height - 2 * margin)
        panel.setFrame(NSRect(x: screen.maxX - size.width - margin, y: screen.maxY - height - margin,
                              width: size.width, height: height), display: true)
        panel.orderFrontRegardless()
    }

    private var wasAsking = false

    private func setButtons(_ buttons: [[String: Any]]) {
        for view in buttonRow.arrangedSubviews {
            buttonRow.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        for button in buttons {
            let key = button["key"] as? String ?? ""
            let label = button["label"] as? String ?? ""
            buttonRow.addArrangedSubview(OverlayPanel.button("\(key) · \(label)", tone: button["tone"] as? String))
        }
        buttonRow.isHidden = buttons.isEmpty
    }

    /// A large, calm button that shows which controller button to press.
    /// It is a picture, not a control: the panel ignores the mouse.
    private static func button(_ text: String, tone: String?) -> NSView {
        let color: NSColor
        switch tone {
        case "yes": color = NSColor(calibratedRed: 0.24, green: 0.55, blue: 0.36, alpha: 0.85)
        case "no": color = NSColor(calibratedRed: 0.66, green: 0.27, blue: 0.27, alpha: 0.85)
        default: color = NSColor.gray.withAlphaComponent(0.6)
        }
        let box = NSView()
        box.wantsLayer = true
        box.layer?.backgroundColor = color.cgColor
        box.layer?.cornerRadius = 10
        let label = NSTextField(labelWithString: text)
        label.font = .systemFont(ofSize: 20, weight: .semibold)
        label.textColor = .white
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(label)
        NSLayoutConstraint.activate([
            box.heightAnchor.constraint(equalToConstant: 52),
            label.centerXAnchor.constraint(equalTo: box.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: box.centerYAnchor),
        ])
        return box
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
