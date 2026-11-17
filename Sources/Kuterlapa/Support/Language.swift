import Foundation

/// The language of what Kuterlapa shows: English, or Polish when the user
/// chose it (`language = "pl"` at the top of config.toml). The core reads
/// the same setting; this app cannot ask the core, so it reads the file
/// itself, once. A change needs a restart.
enum Language {
    static let polish: Bool = {
        let file = Paths.config.appendingPathComponent("config.toml")
        guard let text = try? String(contentsOf: file, encoding: .utf8) else { return false }
        let pattern = #"^\s*language\s*=\s*"([^"]*)""#
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return false }
        for line in text.components(separatedBy: .newlines) {
            // The key counts only at the top, before the first table.
            if line.trimmingCharacters(in: .whitespaces).hasPrefix("[") { break }
            let range = NSRange(line.startIndex..., in: line)
            if let match = regex.firstMatch(in: line, range: range), let value = Range(match.range(at: 1), in: line) {
                return line[value].trimmingCharacters(in: .whitespaces) == "pl"
            }
        }
        return false
    }()
}

/// The Polish or the English text.
func tr(_ pl: String, _ en: String) -> String {
    Language.polish ? pl : en
}
