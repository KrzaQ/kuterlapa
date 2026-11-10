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
    /// What the model does while it works: tool calls and thinking, small.
    private let activityLabel = OverlayPanel.label(size: 11, weight: .regular, color: .secondaryLabelColor)
    private let buttonRow = NSStackView()
    /// The options of a model's question, one under another.
    private let choiceList = NSStackView()
    private let countdownBar = CountdownBar()
    private let badge = Badge()
    private var titleRow: NSStackView!
    /// A dashed line around the panel: something temporary (a one-off
    /// command), unlike the solid frame of a question.
    private let dashes = CAShapeLayer()
    private let helpView = NSStackView()
    private var background: NSVisualEffectView!
    private var widthConstraint: NSLayoutConstraint!
    /// The size of everything in the panel; the user changes it with the
    /// controller (overlay-bigger, overlay-smaller in the core).
    private var scale: CGFloat = 1
    private var lastState: [String: Any]?
    private var width: CGFloat { 440 * scale }
    /// The help needs room for two columns.
    private var helpWidth: CGFloat { (panelWidth ?? 640) * scale }
    /// A panel's own width, before scaling ("width" in the state).
    private var panelWidth: CGFloat?
    private let margin: CGFloat = 12

    init() {
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 440, height: 120),
                        styleMask: [.nonactivatingPanel, .borderless],
                        backing: .buffered, defer: true)
        panel.level = .statusBar
        // Not .stationary: a stationary window stays off the desktops of
        // full-screen apps.
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
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

        choiceList.orientation = .vertical
        choiceList.alignment = .leading
        choiceList.spacing = 6
        choiceList.isHidden = true

        helpView.orientation = .horizontal
        helpView.alignment = .top
        helpView.spacing = 28
        helpView.isHidden = true

        titleRow = NSStackView(views: [badge, titleLabel])
        titleRow.orientation = .horizontal
        titleRow.alignment = .centerY
        titleRow.spacing = 8
        badge.isHidden = true

        let stack = NSStackView(views: [titleRow, statusLabel, countdownBar, headingLabel, textLabel, activityLabel, detailLabel,
                                        helpView, choiceList, buttonRow, hintLabel])
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
            choiceList.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -28),
            countdownBar.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -28),
            countdownBar.heightAnchor.constraint(equalToConstant: 6),
        ])
        widthConstraint = stack.widthAnchor.constraint(equalToConstant: width)
        widthConstraint.isActive = true
        countdownBar.isHidden = true
        setWidth(width)
        dashes.fillColor = nil
        dashes.lineWidth = 1.5
        dashes.lineDashPattern = [7, 5]
        dashes.isHidden = true
        background.layer?.addSublayer(dashes)
        panel.contentView = background
        applyFonts()
    }

    /// The panel's window number, for screenshots of it alone.
    var windowNumber: Int { panel.windowNumber }

    /// The panel's place on screen in points, from the top left corner of
    /// the main display: "x,y,width,height", for screenshots of that area.
    var place: String {
        let f = panel.frame
        let top = (NSScreen.screens.first?.frame.height ?? 0) - f.maxY
        return "\(Int(f.minX)),\(Int(top)),\(Int(f.width)),\(Int(f.height))"
    }

    /// Changes the size of the panel and its text, and redraws it.
    func setScale(_ value: CGFloat) {
        scale = min(max(value, 0.6), 3)
        applyFonts()
        if let state = lastState {
            update(state)
        }
    }

    private func applyFonts() {
        let fonts: [(NSTextField, CGFloat, NSFont.Weight)] = [
            (titleLabel, 12, .semibold), (statusLabel, 12, .regular), (headingLabel, 22, .bold),
            (textLabel, 15, .regular), (hintLabel, 11, .regular), (detailLabel, 12, .regular),
        ]
        for (label, size, weight) in fonts {
            label.font = .systemFont(ofSize: size * scale, weight: weight)
        }
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
    /// A model's question with options: "choices": [{"text": "Do koszyka",
    /// "selected": true}, ...]. The d-pad moves the mark, so the core sends
    /// the state again with another option selected.
    ///
    /// While a model works, what it does, small, below the text:
    /// "activity": [{"kind": "tool", "state": "running" | "done" | "failed",
    ///               "name": "chrome_tabs", "text": "<arguments>"},
    ///              {"kind": "thinking", "text": "..."}, {"kind": "more", "text": "..."}]
    ///
    /// While the user pauses in a chat message, a bar shrinks until the
    /// message sends itself: "countdown": {"ends": <Unix time>, "total": <s>}.
    ///
    /// What kind of exchange this is: "badge": {"text": "ROZMOWA", "tone":
    /// "conversation" | "command"}, and "frame": "dashed" for something
    /// temporary.
    func update(_ state: [String: Any]) {
        lastState = state
        guard state["visible"] as? Bool == true else {
            panel.orderOut(nil)
            return
        }
        set(titleLabel, state["title"])
        set(statusLabel, state["status"])
        set(headingLabel, state["heading"])
        // Answers from a model are Markdown.
        let text = state["text"] as? String ?? ""
        textLabel.attributedStringValue = Markdown.render(text, size: 15 * scale, color: .labelColor)
        textLabel.isHidden = text.isEmpty
        setActivity(state["activity"] as? [[String: Any]] ?? [])
        set(detailLabel, state["detail"])
        set(hintLabel, state["hint"])
        let help = state["help"] as? [[String: Any]] ?? []
        panelWidth = (state["width"] as? Double).map { CGFloat($0) }
        setWidth(help.isEmpty ? width : helpWidth)
        setHelp(help, keyWidth: (state["help_key_width"] as? Double).map { CGFloat($0) } ?? 126,
                oneColumn: state["help_columns"] as? Int == 1)
        setChoices(state["choices"] as? [[String: Any]] ?? [])
        setButtons(state["buttons"] as? [[String: Any]] ?? [])
        if let countdown = state["countdown"] as? [String: Any],
           let ends = countdown["ends"] as? Double, let total = countdown["total"] as? Double, total > 0 {
            countdownBar.start(ends: Date(timeIntervalSince1970: ends), total: total)
        } else {
            countdownBar.stop()
        }
        let kind = state["badge"] as? [String: Any]
        badge.set(text: kind?["text"] as? String ?? "", tone: kind?["tone"] as? String ?? "", scale: scale)
        let dashed = state["frame"] as? String == "dashed"
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
        dashes.isHidden = !dashed || attention
        if dashed {
            dashes.strokeColor = Badge.color("command").withAlphaComponent(0.85).cgColor
            let inset = background.bounds.insetBy(dx: 1.5, dy: 1.5)
            dashes.path = CGPath(roundedRect: inset, cornerWidth: 11, cornerHeight: 11, transform: nil)
        }
        panel.orderFrontRegardless()
    }

    private var wasAsking = false

    private func setWidth(_ value: CGFloat) {
        widthConstraint.constant = value
        for label in [titleLabel, statusLabel, headingLabel, textLabel, activityLabel, detailLabel, hintLabel] {
            label.preferredMaxLayoutWidth = value - 28
        }
    }

    // MARK: Help

    /// The help: groups of rows, each with keys and a short text, in two
    /// columns of about the same length.
    /// "help": [{"title": "Mówienie", "rows": [{"keys": "X ×2", "text": "nowy szkic"}]}]
    /// "help_key_width": the width of the keys column (default 126).
    /// "help_columns": 1 keeps all groups in one column of the whole width.
    private func setHelp(_ groups: [[String: Any]], keyWidth: CGFloat, oneColumn: Bool) {
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
        while !oneColumn && split < groups.count - 1 && filled + sizes[split] / 2 < half {
            filled += sizes[split]
            split += 1
        }
        // One group takes the whole width; more share two columns.
        let whole = groups.count == 1 || oneColumn
        let columnWidth = whole ? helpWidth - 28 : (helpWidth - 28 - helpView.spacing) / 2
        let parts = oneColumn ? [groups[...]] : [groups[..<max(split, 1)], groups[max(split, 1)...]]
        for part in parts where !part.isEmpty {
            let column = NSStackView()
            column.orientation = .vertical
            column.alignment = .leading
            column.spacing = 5
            for group in part {
                let title = OverlayPanel.label(size: 11 * scale, weight: .bold, color: .systemOrange)
                title.stringValue = (group["title"] as? String ?? "").uppercased()
                if column.arrangedSubviews.count > 0 {
                    column.setCustomSpacing(14, after: column.arrangedSubviews.last!)
                }
                column.addArrangedSubview(title)
                column.setCustomSpacing(7, after: title)
                for row in group["rows"] as? [[String: Any]] ?? [] {
                    column.addArrangedSubview(OverlayPanel.helpRow(keys: row["keys"] as? String ?? "",
                                                                   text: row["text"] as? String ?? "",
                                                                   width: columnWidth, keyWidth: keyWidth,
                                                                   scale: scale))
                }
            }
            column.widthAnchor.constraint(equalToConstant: columnWidth).isActive = true
            helpView.addArrangedSubview(column)
        }
    }

    /// One help row: the keys as small key caps, then what they do.
    private static func helpRow(keys: String, text: String, width: CGFloat, keyWidth: CGFloat,
                                scale: CGFloat) -> NSView {
        let caps = NSStackView()
        caps.orientation = .horizontal
        caps.spacing = 4
        for key in keys.components(separatedBy: " · ") {
            caps.addArrangedSubview(keyCap(key, scale: scale))
        }
        let keyColumn: CGFloat = keyWidth * scale
        caps.widthAnchor.constraint(equalToConstant: keyColumn).isActive = true
        caps.setHuggingPriority(.required, for: .horizontal)
        caps.addArrangedSubview(NSView())  // pushes the caps to the left
        let label = OverlayPanel.label(size: 14 * scale, weight: .regular, color: .labelColor)
        label.stringValue = text
        label.preferredMaxLayoutWidth = width - keyColumn - 10
        let row = NSStackView(views: [caps, label])
        row.orientation = .horizontal
        row.alignment = .centerY
        row.spacing = 10
        return row
    }

    /// A key cap: "X", "Y ×2" or "lewa gałka" on a light, rounded plate.
    private static func keyCap(_ text: String, scale: CGFloat) -> NSView {
        let label = NSTextField(labelWithString: text)
        label.font = .systemFont(ofSize: 12 * scale, weight: .semibold)
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
            plate.widthAnchor.constraint(greaterThanOrEqualToConstant: 26 * scale),
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
            buttonRow.addArrangedSubview(OverlayPanel.button("\(key) · \(label)", tone: button["tone"] as? String,
                                                              scale: scale))
        }
        buttonRow.isHidden = buttons.isEmpty
    }

    private func setChoices(_ choices: [[String: Any]]) {
        for view in choiceList.arrangedSubviews {
            choiceList.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        for (i, choice) in choices.enumerated() {
            let row = OverlayPanel.choice("\(i + 1). \(choice["text"] as? String ?? "")",
                                          selected: choice["selected"] as? Bool == true, scale: scale)
            choiceList.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: choiceList.widthAnchor).isActive = true
        }
        choiceList.isHidden = choices.isEmpty
    }

    /// One option: the marked one stands out with a strong frame and color.
    private static func choice(_ text: String, selected: Bool, scale: CGFloat) -> NSView {
        let box = NSView()
        box.wantsLayer = true
        box.layer?.cornerRadius = 9
        box.layer?.backgroundColor = selected
            ? NSColor(calibratedRed: 0.95, green: 0.62, blue: 0.25, alpha: 0.9).cgColor
            : NSColor.white.withAlphaComponent(0.08).cgColor
        box.layer?.borderWidth = selected ? 2 : 1
        box.layer?.borderColor = selected ? NSColor.white.withAlphaComponent(0.85).cgColor
                                          : NSColor.white.withAlphaComponent(0.15).cgColor
        let label = NSTextField(wrappingLabelWithString: (selected ? "▶ " : "   ") + text)
        label.font = .systemFont(ofSize: 18 * scale, weight: selected ? .bold : .regular)
        label.textColor = selected ? .black : .labelColor
        label.isSelectable = false
        label.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: 12 * scale),
            label.trailingAnchor.constraint(equalTo: box.trailingAnchor, constant: -12 * scale),
            label.topAnchor.constraint(equalTo: box.topAnchor, constant: 9 * scale),
            label.bottomAnchor.constraint(equalTo: box.bottomAnchor, constant: -9 * scale),
        ])
        return box
    }

    /// A large, calm button that shows which controller button to press.
    /// It is a picture, not a control: the panel ignores the mouse.
    private static func button(_ text: String, tone: String?, scale: CGFloat) -> NSView {
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
        label.font = .systemFont(ofSize: 20 * scale, weight: .semibold)
        label.textColor = .white
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(label)
        NSLayoutConstraint.activate([
            box.heightAnchor.constraint(equalToConstant: 52 * scale),
            label.centerXAnchor.constraint(equalTo: box.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: box.centerYAnchor),
        ])
        return box
    }

    private func setActivity(_ lines: [[String: Any]]) {
        activityLabel.isHidden = lines.isEmpty
        guard !lines.isEmpty else { return }
        let size = 11 * scale
        let plain = NSFont.systemFont(ofSize: size)
        let mono = NSFont.monospacedSystemFont(ofSize: size, weight: .regular)
        let bold = NSFont.monospacedSystemFont(ofSize: size, weight: .semibold)
        let italic = NSFontManager.shared.convert(plain, toHaveTrait: .italicFontMask)
        let paragraph = NSMutableParagraphStyle()
        paragraph.paragraphSpacing = 3 * scale
        let out = NSMutableAttributedString()
        func add(_ text: String, _ font: NSFont, _ color: NSColor) {
            out.append(NSAttributedString(string: text, attributes: [
                .font: font, .foregroundColor: color, .paragraphStyle: paragraph,
            ]))
        }
        for (i, line) in lines.enumerated() {
            if i > 0 { add("\n", plain, .secondaryLabelColor) }
            let text = line["text"] as? String ?? ""
            switch line["kind"] as? String {
            case "tool":
                let (mark, color): (String, NSColor) = switch line["state"] as? String {
                case "done": ("✓", .systemGreen)
                case "failed": ("✗", .systemRed)
                default: ("⋯", .systemOrange)
                }
                add("\(mark) ", bold, color)
                add(line["name"] as? String ?? "", bold, .labelColor.withAlphaComponent(0.8))
                if !text.isEmpty { add("  \(text)", mono, .secondaryLabelColor) }
            case "thinking":
                add("💭 ", plain, .secondaryLabelColor)
                add(text, italic, .secondaryLabelColor)
            default:
                add(text, plain, .tertiaryLabelColor)
            }
        }
        activityLabel.attributedStringValue = out
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

/// A small colored label before the title: what kind of exchange the panel
/// shows (a saved conversation, a one-off command).
final class Badge: NSView {
    private let label = NSTextField(labelWithString: "")

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.cornerRadius = 5
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 7),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -7),
            label.topAnchor.constraint(equalTo: topAnchor, constant: 2),
            label.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -2),
        ])
        setContentHuggingPriority(.required, for: .horizontal)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    static func color(_ tone: String) -> NSColor {
        switch tone {
        case "command": return NSColor(calibratedRed: 0.50, green: 0.69, blue: 1.0, alpha: 1)
        default: return NSColor.systemOrange
        }
    }

    func set(text: String, tone: String, scale: CGFloat) {
        isHidden = text.isEmpty
        guard !text.isEmpty else { return }
        let color = Badge.color(tone)
        label.stringValue = text
        label.font = .systemFont(ofSize: 10 * scale, weight: .bold)
        label.textColor = color
        layer?.backgroundColor = color.withAlphaComponent(0.18).cgColor
        layer?.borderWidth = 1
        layer?.borderColor = color.withAlphaComponent(0.45).cgColor
    }
}
