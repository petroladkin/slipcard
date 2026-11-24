import Foundation

/// Anki's rendered card HTML as the plain text the popup shows: style and
/// script blocks, tags, images and sound references dropped, line breaks
/// kept from <br>, <div>, <p>, <li> and the like, entities decoded, spaces
/// collapsed, empty lines removed. The Linux app has the same scanner
/// (linux/src/card_text.rs), and both read testdata/ankiconnect/card-text.json.
public enum CardText {
    static let breakingTags: Set<String> = [
        "br", "div", "p", "li", "tr", "hr", "h1", "h2", "h3", "h4", "h5", "h6",
    ]
    static let skippedBlocks: Set<String> = ["style", "script"]
    /// Sound references: as stored in a field, and as Anki renders them.
    static let soundPrefixes = ["[sound:", "[anki:play:"]

    public static func plain(_ html: String) -> String {
        let chars = Array(html)
        let n = chars.count
        var out = ""
        var i = 0

        func lowered(_ from: Int, _ count: Int) -> String {
            String(chars[from..<min(from + count, n)]).lowercased()
        }

        func find(_ c: Character, from: Int) -> Int? {
            var j = from
            while j < n {
                if chars[j] == c { return j }
                j += 1
            }
            return nil
        }

        scan: while i < n {
            let c = chars[i]
            if c == "<", let end = find(">", from: i + 1) {
                let tag = String(chars[(i + 1)..<end]).trimmingCharacters(in: .whitespaces).lowercased()
                let closing = tag.hasPrefix("/")
                let name = String(
                    tag.drop { $0 == "/" }.prefix { $0.isLetter || $0.isNumber })
                if !closing && skippedBlocks.contains(name) {
                    // Skip to the matching closing tag, or to the end.
                    var j = end + 1
                    while j < n {
                        if chars[j] == "<", lowered(j, name.count + 2) == "</\(name)",
                            let close = find(">", from: j)
                        {
                            i = close + 1
                            continue scan
                        }
                        j += 1
                    }
                    break scan
                }
                if breakingTags.contains(name) { out.append("\n") }
                i = end + 1
                continue
            }
            if c == "[", let prefix = soundPrefixes.first(where: { lowered(i, $0.count) == $0 }),
                let end = find("]", from: i + prefix.count)
            {
                i = end + 1
                continue
            }
            if c == "&", let semi = find(";", from: i + 1), semi - i <= 10,
                let decoded = entity(String(chars[(i + 1)..<semi]))
            {
                out.append(decoded)
                i = semi + 1
                continue
            }
            // A line break in the source is a space, as in HTML; only the
            // breaking tags make lines.
            out.append(c.isNewline ? " " : c)
            i += 1
        }

        return out.split(separator: "\n", omittingEmptySubsequences: false)
            .map { line in
                line.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
            }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
    }

    /// The text of an entity without its & and ;, or nil if unknown.
    static func entity(_ name: String) -> String? {
        switch name {
        case "amp": return "&"
        case "lt": return "<"
        case "gt": return ">"
        case "quot": return "\""
        case "apos", "#39": return "'"
        case "nbsp": return " "
        default:
            let scalar: UInt32?
            if name.hasPrefix("#x") || name.hasPrefix("#X") {
                scalar = UInt32(name.dropFirst(2), radix: 16)
            } else if name.hasPrefix("#") {
                scalar = UInt32(name.dropFirst())
            } else {
                scalar = nil
            }
            return scalar.flatMap(Unicode.Scalar.init).map { String(Character($0)) }
        }
    }
}
