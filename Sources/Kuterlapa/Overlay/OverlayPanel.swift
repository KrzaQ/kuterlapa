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
    private let countdownBar = CountdownBar()
    private let helpView = NSStackView()
    private var background: NSVisualEffectView!
    private var widthConstraint: NSLayoutConstraint!
    private let width: CGFloat = 440
    /// The help needs room for two columns.
    private let helpWidth: CGFloat = 640
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

        helpView.orientation = .horizontal
        helpView.alignment = .top
        helpView.spacing = 28
        helpView.isHidden = true

        let stack = NSStackView(views: [titleLabel, statusLabel, countdownBar, headingLabel, textLabel, detailLabel,
                                        helpView, buttonRow, hintLabel])
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
            buttonRow.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -28),
            countdownBar.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -28),
            countdownBar.heightAnchor.constraint(equalToConstant: 6),
        ])
        widthConstraint = stack.widthAnchor.constraint(equalToConstant: width)
        widthConstraint.isActive = true
        countdownBar.isHidden = true
        setWidth(width)
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
    ///
    /// While the user pauses in a chat message, a bar shrinks until the
    /// message sends itself: "countdown": {"ends": <Unix time>, "total": <s>}.
    func update(_ state: [String: Any]) {
        guard state["visible"] as? Bool == true else {
            panel.orderOut(nil)
            return
        }
        set(titleLabel, state["title"])
        set(statusLabel, state["status"])
        set(headingLabel, state["heading"])
        // Answers from a model are Markdown.
        let text = state["text"] as? String ?? ""
        textLabel.attributedStringValue = Markdown.render(text, size: 15, color: .labelColor)
        textLabel.isHidden = text.isEmpty
        set(detailLabel, state["detail"])
        set(hintLabel, state["hint"])
        let help = state["help"] as? [[String: Any]] ?? []
        setWidth(help.isEmpty ? width : helpWidth)
        setHelp(help)
        setButtons(state["buttons"] as? [[String: Any]] ?? [])
        if let countdown = state["countdown"] as? [String: Any],
           let ends = countdown["ends"] as? Double, let total = countdown["total"] as? Double, total > 0 {
            countdownBar.start(ends: Date(timeIntervalSince1970: ends), total: total)
        } else {
            countdownBar.stop()
        }
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

    private func setWidth(_ value: CGFloat) {
        widthConstraint.constant = value
        for label in [titleLabel, statusLabel, headingLabel, textLabel, detailLabel, hintLabel] {
            label.preferredMaxLayoutWidth = value - 28
        }
    }

    // MARK: Help

    /// The help: groups of rows, each with keys and a short text, in two
    /// columns of about the same length.
    /// "help": [{"title": "Mówienie", "rows": [{"keys": "X ×2", "text": "nowy szkic"}]}]
    private func setHelp(_ groups: [[String: Any]]) {
        for view in helpView.arrangedSubviews {
            helpView.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        helpView.isHidden = groups.isEmpty
        guard !groups.isEmpty else { return }
        let sizes = groups.map { ($0["rows"] as? [[String: Any]])?.count ?? 0 }
        // Fill the left column up to about half of all rows.
        let half = (sizes.reduce(0, +) + 1) / 2
        var split = 0, filled = 0
        while split < groups.count - 1 && filled + sizes[split] / 2 < half {
            filled += sizes[split]
            split += 1
        }
        let columnWidth = (helpWidth - 28 - helpView.spacing) / 2
        for part in [groups[..<max(split, 1)], groups[max(split, 1)...]] where !part.isEmpty {
            let column = NSStackView()
            column.orientation = .vertical
            column.alignment = .leading
            column.spacing = 5
            for group in part {
                let title = OverlayPanel.label(size: 11, weight: .bold, color: .systemOrange)
                title.stringValue = (group["title"] as? String ?? "").uppercased()
                if column.arrangedSubviews.count > 0 {
                    column.setCustomSpacing(14, after: column.arrangedSubviews.last!)
                }
                column.addArrangedSubview(title)
                column.setCustomSpacing(7, after: title)
                for row in group["rows"] as? [[String: Any]] ?? [] {
                    column.addArrangedSubview(OverlayPanel.helpRow(keys: row["keys"] as? String ?? "",
                                                                   text: row["text"] as? String ?? "",
                                                                   width: columnWidth))
                }
            }
            column.widthAnchor.constraint(equalToConstant: columnWidth).isActive = true
            helpView.addArrangedSubview(column)
        }
    }

    /// One help row: the keys as small key caps, then what they do.
    private static func helpRow(keys: String, text: String, width: CGFloat) -> NSView {
        let caps = NSStackView()
        caps.orientation = .horizontal
        caps.spacing = 4
        for key in keys.components(separatedBy: " · ") {
            caps.addArrangedSubview(keyCap(key))
        }
        let keyColumn: CGFloat = 126
        caps.widthAnchor.constraint(equalToConstant: keyColumn).isActive = true
        caps.setHuggingPriority(.required, for: .horizontal)
        caps.addArrangedSubview(NSView())  // pushes the caps to the left
        let label = OverlayPanel.label(size: 14, weight: .regular, color: .labelColor)
        label.stringValue = text
        label.preferredMaxLayoutWidth = width - keyColumn - 10
        let row = NSStackView(views: [caps, label])
        row.orientation = .horizontal
        row.alignment = .centerY
        row.spacing = 10
        return row
    }

    /// A key cap: "X", "Y ×2" or "lewa gałka" on a light, rounded plate.
    private static func keyCap(_ text: String) -> NSView {
        let label = NSTextField(labelWithString: text)
        label.font = .systemFont(ofSize: 12, weight: .semibold)
        label.textColor = .labelColor
        label.translatesAutoresizingMaskIntoConstraints = false
        let plate = NSView()
        plate.wantsLayer = true
        plate.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.16).cgColor
        plate.layer?.cornerRadius = 5
        plate.layer?.borderWidth = 1
        plate.layer?.borderColor = NSColor.white.withAlphaComponent(0.22).cgColor
        plate.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: plate.leadingAnchor, constant: 7),
            label.trailingAnchor.constraint(equalTo: plate.trailingAnchor, constant: -7),
            label.topAnchor.constraint(equalTo: plate.topAnchor, constant: 2),
            label.bottomAnchor.constraint(equalTo: plate.bottomAnchor, constant: -2),
            plate.widthAnchor.constraint(greaterThanOrEqualToConstant: 26),
        ])
        label.alignment = .center
        return plate
    }

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

/// A thin bar that shrinks from full to empty, 30 times a second, until a
/// moment. It keeps its pace when the overlay updates during the countdown.
final class CountdownBar: NSView {
    private let fill = CALayer()
    private var timer: Timer?
    private var ends = Date()
    private var total: TimeInterval = 1

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.cornerRadius = 3
        layer?.backgroundColor = NSColor.white.withAlphaComponent(0.12).cgColor
        fill.backgroundColor = NSColor.systemOrange.withAlphaComponent(0.85).cgColor
        fill.cornerRadius = 3
        fill.anchorPoint = .zero
        layer?.addSublayer(fill)
        translatesAutoresizingMaskIntoConstraints = false
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func start(ends: Date, total: TimeInterval) {
        self.ends = ends
        self.total = total
        isHidden = false
        guard timer == nil else { return }
        let timer = Timer(timeInterval: 1.0 / 30, repeats: true) { [weak self] _ in self?.tick() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
        tick()
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        isHidden = true
    }

    private func tick() {
        let left = max(0, min(1, ends.timeIntervalSinceNow / total))
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fill.frame = CGRect(x: 0, y: 0, width: bounds.width * left, height: bounds.height)
        CATransaction.commit()
    }
}
