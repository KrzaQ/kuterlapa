import AppKit

/// Turns the Markdown of a model's answer into styled text for the
/// overlay. The block structure (headings, lists, code blocks) is handled
/// line by line here; inline styles (bold, italic, code, links) come from
/// Foundation's Markdown parser. AppKit does not draw the parser's
/// presentation intents by itself, so they become fonts here.
///
/// Tables keep the normal font, with their columns at measured tab stops.
/// `width` is the room for the text: a wider table gets a smaller font,
/// and at the smallest one its widest cells are cut.
enum Markdown {
    static func render(_ text: String, size: CGFloat, color: NSColor, width: CGFloat? = nil) -> NSAttributedString {
        let out = NSMutableAttributedString()
        var inCode = false
        let lines = text.components(separatedBy: "\n")
        var index = 0
        while index < lines.count {
            let line = lines[index]
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            defer { index += 1 }
            if trimmed.hasPrefix("```") {
                inCode.toggle()
                continue
            }
            // A table: a row, a separator ("|---|---|"), more rows.
            if !inCode, isRow(trimmed), index + 1 < lines.count, isSeparator(lines[index + 1]) {
                var rows = [cells(trimmed)]
                var next = index + 2
                while next < lines.count, isRow(lines[next].trimmingCharacters(in: .whitespaces)) {
                    rows.append(cells(lines[next].trimmingCharacters(in: .whitespaces)))
                    next += 1
                }
                out.append(table(rows, size: size, color: color, width: width))
                index = next - 1
                if next < lines.count {
                    out.append(NSAttributedString(string: "\n", attributes: [.font: NSFont.systemFont(ofSize: size)]))
                }
                continue
            }
            let piece: NSAttributedString
            if inCode {
                piece = NSAttributedString(string: line, attributes: [
                    .font: NSFont.monospacedSystemFont(ofSize: size - 2, weight: .regular),
                    .foregroundColor: NSColor.secondaryLabelColor,
                ])
            } else if let heading = headingText(trimmed) {
                piece = inline(heading, size: size + 1, color: color, bold: true)
            } else if let (marker, rest, indent) = listItem(line) {
                let item = NSMutableAttributedString(string: marker, attributes: [
                    .font: NSFont.systemFont(ofSize: size), .foregroundColor: NSColor.secondaryLabelColor,
                ])
                item.append(inline(rest, size: size, color: color))
                // Wrapped lines of an item start under its text, not its marker.
                let style = NSMutableParagraphStyle()
                let markerWidth = (marker as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: size)]).width
                style.firstLineHeadIndent = indent
                style.headIndent = indent + markerWidth
                item.addAttribute(.paragraphStyle, value: style, range: NSRange(location: 0, length: item.length))
                piece = item
            } else {
                piece = inline(line, size: size, color: color)
            }
            out.append(piece)
            if index < lines.count - 1 {
                out.append(NSAttributedString(string: "\n", attributes: [.font: NSFont.systemFont(ofSize: size)]))
            }
        }
        return out
    }

    private static func isRow(_ line: String) -> Bool {
        line.hasPrefix("|") && line.count > 1
    }

    private static func isSeparator(_ line: String) -> Bool {
        let t = line.trimmingCharacters(in: .whitespaces)
        return t.hasPrefix("|") && t.contains("-") && t.allSatisfy { "|-: ".contains($0) }
    }

    /// The cells of a row, without the inline Markdown marks: a grid has no
    /// room for styles inside a cell.
    private static func cells(_ row: String) -> [String] {
        var inner = row
        if inner.hasPrefix("|") { inner.removeFirst() }
        if inner.hasSuffix("|") { inner.removeLast() }
        return inner.components(separatedBy: "|").map { cell in
            var text = cell.trimmingCharacters(in: .whitespaces)
            for mark in ["**", "__", "`"] { text = text.replacingOccurrences(of: mark, with: "") }
            text = text.replacingOccurrences(of: "<br>", with: " ")
            return text
        }
    }

    /// The rows as a table: columns at tab stops measured for this font, so
    /// they line up in the normal proportional font. The first row is bold,
    /// with a line under it. A table wider than the room gets a smaller font
    /// (down to 10 points), and then its widest cells are cut.
    private static func table(_ rows: [[String]], size: CGFloat, color: NSColor, width: CGFloat?) -> NSAttributedString {
        let count = rows.map(\.count).max() ?? 0
        guard count > 0 else { return NSAttributedString() }
        var rows = rows.map { $0 + Array(repeating: "", count: count - $0.count) }
        let room = width ?? .greatestFiniteMagnitude
        func font(_ size: CGFloat, _ row: Int) -> NSFont {
            .systemFont(ofSize: size, weight: row == 0 ? .semibold : .regular)
        }
        func measure(_ text: String, _ size: CGFloat, _ row: Int) -> CGFloat {
            ceil((text as NSString).size(withAttributes: [.font: font(size, row)]).width)
        }
        func columns(_ size: CGFloat) -> [CGFloat] {
            (0..<count).map { c in rows.indices.map { r in measure(rows[r][c], size, r) }.max() ?? 0 }
        }
        var fontSize = size - 1
        var gap = round(fontSize * 1.1)
        var widths = columns(fontSize)
        while fontSize > 10, widths.reduce(0, +) + gap * CGFloat(count - 1) > room {
            fontSize -= 1
            gap = round(fontSize * 1.1)
            widths = columns(fontSize)
        }
        // Still too wide: cut the cells of the widest column.
        while widths.reduce(0, +) + gap * CGFloat(count - 1) > room,
              let widest = widths.indices.max(by: { widths[$0] < widths[$1] }), widths[widest] > fontSize * 3 {
            let limit = widths[widest] - fontSize
            for r in rows.indices where measure(rows[r][widest], fontSize, r) > limit {
                var text = rows[r][widest].hasSuffix("…") ? String(rows[r][widest].dropLast()) : rows[r][widest]
                while !text.isEmpty, measure(text + "…", fontSize, r) > limit { text.removeLast() }
                rows[r][widest] = text + "…"
            }
            widths = columns(fontSize)
        }
        let style = NSMutableParagraphStyle()
        var position: CGFloat = 0
        style.tabStops = widths.dropLast().map { w in
            position += w + gap
            return NSTextTab(textAlignment: .left, location: position)
        }
        style.lineBreakMode = .byClipping
        let out = NSMutableAttributedString()
        func add(_ text: String, _ font: NSFont, _ tone: NSColor) {
            out.append(NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: tone, .paragraphStyle: style]))
        }
        let total = widths.reduce(0, +) + gap * CGFloat(count - 1)
        for (r, row) in rows.enumerated() {
            if r > 0 { add("\n", font(fontSize, r), color) }
            add(row.joined(separator: "\t"), font(fontSize, r), color)
            if r == 0 {
                // A thin line as wide as the table.
                let dash = NSFont.systemFont(ofSize: fontSize * 0.7)
                let dashWidth = ("─" as NSString).size(withAttributes: [.font: dash]).width
                add("\n" + String(repeating: "─", count: max(Int((total / max(dashWidth, 1)).rounded(.up)), 1)), dash, .tertiaryLabelColor)
            }
        }
        return out
    }

    private static func headingText(_ line: String) -> String? {
        guard line.hasPrefix("#") else { return nil }
        let text = line.drop { $0 == "#" }
        guard text.hasPrefix(" ") else { return nil }
        return text.trimmingCharacters(in: .whitespaces)
    }

    /// "- text", "* text", "+ text" or "1. text": the marker to show, the
    /// text and the indent of nested items.
    private static func listItem(_ line: String) -> (String, String, CGFloat)? {
        let spaces = line.prefix { $0 == " " }.count
        let indent = CGFloat(spaces / 2) * 16
        let rest = line.dropFirst(spaces)
        for bullet in ["- ", "* ", "+ "] where rest.hasPrefix(bullet) {
            return ("•  ", String(rest.dropFirst(2)), indent)
        }
        let digits = rest.prefix { $0.isNumber }
        if !digits.isEmpty, rest.dropFirst(digits.count).hasPrefix(". ") {
            return ("\(digits).  ", String(rest.dropFirst(digits.count + 2)), indent)
        }
        return nil
    }

    private static func inline(_ text: String, size: CGFloat, color: NSColor, bold: Bool = false) -> NSAttributedString {
        let base = NSFont.systemFont(ofSize: size, weight: bold ? .semibold : .regular)
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        guard let parsed = try? AttributedString(markdown: text, options: options) else {
            return NSAttributedString(string: text, attributes: [.font: base, .foregroundColor: color])
        }
        let out = NSMutableAttributedString(parsed)
        let whole = NSRange(location: 0, length: out.length)
        out.addAttributes([.font: base, .foregroundColor: color], range: whole)
        out.enumerateAttribute(.inlinePresentationIntent, in: whole) { value, range, _ in
            guard let raw = value as? UInt else { return }
            let intent = InlinePresentationIntent(rawValue: raw)
            var font = base
            if intent.contains(.code) {
                font = .monospacedSystemFont(ofSize: size - 1, weight: bold ? .semibold : .regular)
                out.addAttribute(.backgroundColor, value: NSColor.white.withAlphaComponent(0.10), range: range)
            }
            var traits: NSFontDescriptor.SymbolicTraits = []
            if intent.contains(.stronglyEmphasized) { traits.insert(.bold) }
            if intent.contains(.emphasized) { traits.insert(.italic) }
            if !traits.isEmpty {
                let descriptor = font.fontDescriptor.withSymbolicTraits(font.fontDescriptor.symbolicTraits.union(traits))
                font = NSFont(descriptor: descriptor, size: font.pointSize) ?? font
            }
            if intent.contains(.strikethrough) {
                out.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
            }
            out.addAttribute(.font, value: font, range: range)
        }
        out.enumerateAttribute(.link, in: whole) { value, range, _ in
            if value != nil {
                out.addAttribute(.foregroundColor, value: NSColor.linkColor, range: range)
                out.removeAttribute(.link, range: range)
            }
        }
        return out
    }
}
