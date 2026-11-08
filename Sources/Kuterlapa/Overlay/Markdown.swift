import AppKit

/// Turns the Markdown of a model's answer into styled text for the
/// overlay. The block structure (headings, lists, code blocks) is handled
/// line by line here; inline styles (bold, italic, code, links) come from
/// Foundation's Markdown parser. AppKit does not draw the parser's
/// presentation intents by itself, so they become fonts here.
enum Markdown {
    static func render(_ text: String, size: CGFloat, color: NSColor) -> NSAttributedString {
        let out = NSMutableAttributedString()
        var inCode = false
        let lines = text.components(separatedBy: "\n")
        for (index, line) in lines.enumerated() {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") {
                inCode.toggle()
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
