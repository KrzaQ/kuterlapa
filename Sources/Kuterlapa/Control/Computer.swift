import AppKit
import ApplicationServices

/// Lets the model see and use the computer: screenshots, the elements of
/// the window in front, clicks and keys. The core's MCP server calls these
/// through the shell socket (requests "screen", "ui", "press", ...).
///
/// The model sees one coordinate space, with the origin in the top left
/// corner of the main display: screen points times `scale`, so the screen
/// is at most `modelWidth` wide. Screenshots have that size, element
/// positions use it, and clicks and scrolls come in it. A smaller picture
/// costs the model less time.
enum Computer {
    static let modelWidth: CGFloat = 1024

    static var scale: CGFloat {
        let width = NSScreen.main?.frame.width ?? modelWidth
        return min(1, modelWidth / width)
    }

    /// From the model's coordinates to screen points.
    static func point(_ x: Double, _ y: Double) -> CGPoint {
        CGPoint(x: CGFloat(x) / scale, y: CGFloat(y) / scale)
    }

    /// Elements from the last "ui" request, by number, for "press".
    private static var elements: [Int: AXUIElement] = [:]

    private static var folder: URL {
        let dir = Paths.state.appendingPathComponent("computer", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    // MARK: Seeing

    /// Captures the main display in the model's coordinates, as JPEG.
    /// Returns "<path> <width> <height>".
    static func screenshot() -> String {
        let raw = folder.appendingPathComponent("screen-raw.png")
        let out = folder.appendingPathComponent("screen.jpg")
        let capture = Process()
        capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        capture.arguments = ["-x", "-m", "-t", "png", raw.path]
        do {
            try capture.run()
            capture.waitUntilExit()
        } catch {
            return "error: screencapture: \(error.localizedDescription)"
        }
        guard capture.terminationStatus == 0,
              let image = NSImage(contentsOf: raw),
              let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
        else {
            return "error: no screenshot (does Kuterlapa have the Screen Recording permission?)"
        }
        let size = NSScreen.main?.frame.size ?? CGSize(width: cg.width, height: cg.height)
        let width = Int(size.width * scale), height = Int(size.height * scale)
        guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                                      bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)
        else { return "error: cannot scale the screenshot" }
        context.interpolationQuality = .high
        context.draw(cg, in: CGRect(x: 0, y: 0, width: width, height: height))
        guard let scaled = context.makeImage(),
              let jpeg = NSBitmapImageRep(cgImage: scaled)
                .representation(using: .jpeg, properties: [.compressionFactor: 0.8])
        else { return "error: cannot encode the screenshot" }
        do {
            try jpeg.write(to: out)
        } catch {
            return "error: \(error.localizedDescription)"
        }
        try? FileManager.default.removeItem(at: raw)
        return "\(out.path) \(width) \(height)"
    }

    /// Writes the usable elements of the window in front to a file, one per
    /// line, numbered for "press". Returns the file's path.
    static func ui() -> String {
        guard let app = NSWorkspace.shared.frontmostApplication else { return "error: no application in front" }
        let root = AXUIElementCreateApplication(app.processIdentifier)
        // Chromium and Electron apps build their tree only for assistive
        // technology that asks for it.
        AXUIElementSetAttributeValue(root, "AXManualAccessibility" as CFString, kCFBooleanTrue)
        elements = [:]
        var lines = ["Application: \(app.localizedName ?? "?") (\(app.bundleIdentifier ?? "?"))"]
        let window: AXUIElement? = attribute(root, kAXFocusedWindowAttribute)
            ?? (attribute(root, kAXWindowsAttribute) as [AXUIElement]?)?.first
        guard let window else { return write(lines + ["(no window)"]) }
        lines.append("Window: \(text(window) ?? "")")
        var count = 0
        walk(window, depth: 0, lines: &lines, count: &count)
        if count >= maxElements { lines.append("(more elements not listed)") }
        return write(lines)
    }

    private static let maxElements = 400
    private static let usable: Set<String> = [
        "AXButton", "AXLink", "AXTextField", "AXTextArea", "AXSearchField", "AXCheckBox",
        "AXRadioButton", "AXPopUpButton", "AXComboBox", "AXMenuButton", "AXMenuItem", "AXTab",
        "AXSlider", "AXIncrementor", "AXDisclosureTriangle", "AXCell", "AXRow", "AXStaticText",
        "AXHeading", "AXImage",
    ]

    private static func walk(_ element: AXUIElement, depth: Int, lines: inout [String], count: inout Int) {
        guard depth < 40, count < maxElements else { return }
        let role: String = attribute(element, kAXRoleAttribute) ?? ""
        if usable.contains(role), let frame = frame(element), frame.width > 1, frame.height > 1 {
            let label = text(element) ?? ""
            // Text and pictures without words say nothing to the model.
            if !(["AXStaticText", "AXImage", "AXRow", "AXCell"].contains(role) && label.isEmpty) {
                count += 1
                elements[count] = element
                let name = role.dropFirst(2)
                let shown = label.isEmpty ? "" : " \"\(label.prefix(120))\""
                let k = scale
                lines.append("[\(count)] \(name)\(shown) at \(Int(frame.midX * k)),\(Int(frame.midY * k)) "
                             + "size \(Int(frame.width * k))x\(Int(frame.height * k))")
            }
        }
        let children: [AXUIElement] = attribute(element, kAXChildrenAttribute) ?? []
        for child in children {
            walk(child, depth: depth + 1, lines: &lines, count: &count)
        }
    }

    private static func text(_ element: AXUIElement) -> String? {
        for key in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute, kAXHelpAttribute] {
            if let value: String = attribute(element, key), !value.trimmingCharacters(in: .whitespaces).isEmpty {
                return value.replacingOccurrences(of: "\n", with: " ")
            }
        }
        return nil
    }

    private static func frame(_ element: AXUIElement) -> CGRect? {
        guard let position: AXValue = attribute(element, kAXPositionAttribute),
              let size: AXValue = attribute(element, kAXSizeAttribute)
        else { return nil }
        var point = CGPoint.zero, extent = CGSize.zero
        AXValueGetValue(position, .cgPoint, &point)
        AXValueGetValue(size, .cgSize, &extent)
        return CGRect(origin: point, size: extent)
    }

    private static func attribute<T>(_ element: AXUIElement, _ name: String) -> T? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
        return value as? T
    }

    private static func write(_ lines: [String]) -> String {
        let out = folder.appendingPathComponent("ui.txt")
        do {
            try lines.joined(separator: "\n").write(to: out, atomically: true, encoding: .utf8)
        } catch {
            return "error: \(error.localizedDescription)"
        }
        return out.path
    }

    // MARK: Acting

    /// Presses element `number` from the last "ui" list. If the element
    /// has no press action, clicks its middle.
    static func press(_ number: Int) -> String {
        guard let element = elements[number] else { return "error: no element \(number); ask for ui first" }
        if AXUIElementPerformAction(element, kAXPressAction as CFString) == .success { return "ok" }
        guard let frame = frame(element) else { return "error: element \(number) has no position" }
        return click(at: CGPoint(x: frame.midX, y: frame.midY), how: "click")
    }

    /// Clicks at a point: "click", "double-click" or "right-click".
    static func click(at point: CGPoint, how: String) -> String {
        CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point,
                mouseButton: .left)?.post(tap: .cghidEventTap)
        usleep(30_000)
        switch how {
        case "click": Mouse.click()
        case "double-click": Mouse.click(count: 2)
        case "right-click": Mouse.click(.right)
        default: return "error: unknown click \(how)"
        }
        return "ok"
    }

    /// Scrolls at a point by pixels; positive dy scrolls the content up
    /// (shows what is above).
    static func scroll(at point: CGPoint, dx: Int32, dy: Int32) -> String {
        CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point,
                mouseButton: .left)?.post(tap: .cghidEventTap)
        usleep(30_000)
        Mouse.scroll(dx: dx, dy: dy)
        return "ok"
    }

    /// The running applications with windows, one per line; "*" marks the
    /// one in front. One line for the socket: lines joined with " | ".
    static func apps() -> String {
        let front = NSWorkspace.shared.frontmostApplication?.processIdentifier
        return NSWorkspace.shared.runningApplications
            .filter { $0.activationPolicy == .regular }
            .map { ($0.processIdentifier == front ? "* " : "") + ($0.localizedName ?? "?") }
            .joined(separator: " | ")
    }

    /// Brings an application to the front by name, or opens it.
    static func activate(_ name: String) -> String {
        let wanted = name.lowercased()
        if let app = NSWorkspace.shared.runningApplications.first(where: {
            $0.activationPolicy == .regular && ($0.localizedName?.lowercased() == wanted
                || $0.bundleIdentifier?.lowercased() == wanted)
        }) {
            app.activate()
            return "ok"
        }
        let open = Process()
        open.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        open.arguments = ["-a", name]
        do {
            try open.run()
            open.waitUntilExit()
        } catch {
            return "error: \(error.localizedDescription)"
        }
        return open.terminationStatus == 0 ? "ok" : "error: no application named \(name)"
    }
}
