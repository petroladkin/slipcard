//! Anki's rendered card HTML as the plain text the popup shows: style and
//! script blocks, tags, images and sound references dropped, line breaks
//! kept from <br>, <div>, <p>, <li> and the like, entities decoded, spaces
//! collapsed, empty lines removed. The macOS app has the same scanner
//! (CardText.swift), and both read testdata/ankiconnect/card-text.json.

const BREAKING_TAGS: [&str; 12] = ["br", "div", "p", "li", "tr", "hr", "h1", "h2", "h3", "h4", "h5", "h6"];
const SKIPPED_BLOCKS: [&str; 2] = ["style", "script"];
/// Sound references: as stored in a field, and as Anki renders them.
const SOUND_PREFIXES: [&str; 2] = ["[sound:", "[anki:play:"];

pub fn plain(html: &str) -> String {
    let chars: Vec<char> = html.chars().collect();
    let n = chars.len();
    let lowered = |from: usize, count: usize| -> String {
        chars[from..(from + count).min(n)].iter().collect::<String>().to_lowercase()
    };
    let find = |c: char, from: usize| (from..n).find(|&j| chars[j] == c);
    let mut out = String::new();
    let mut i = 0;
    'scan: while i < n {
        let c = chars[i];
        if c == '<' {
            if let Some(end) = find('>', i + 1) {
                let tag: String = chars[i + 1..end].iter().collect::<String>().trim().to_lowercase();
                let closing = tag.starts_with('/');
                let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_alphanumeric()).collect();
                if !closing && SKIPPED_BLOCKS.contains(&name.as_str()) {
                    // Skip to the matching closing tag, or to the end.
                    let close_tag = format!("</{name}");
                    let mut j = end + 1;
                    while j < n {
                        if chars[j] == '<' && lowered(j, close_tag.chars().count()) == close_tag {
                            if let Some(close) = find('>', j) {
                                i = close + 1;
                                continue 'scan;
                            }
                        }
                        j += 1;
                    }
                    break 'scan;
                }
                if BREAKING_TAGS.contains(&name.as_str()) {
                    out.push('\n');
                }
                i = end + 1;
                continue;
            }
        }
        if c == '[' {
            if let Some(prefix) = SOUND_PREFIXES.iter().find(|p| lowered(i, p.len()) == **p) {
                if let Some(end) = find(']', i + prefix.len()) {
                    i = end + 1;
                    continue;
                }
            }
        }
        if c == '&' {
            if let Some(semi) = find(';', i + 1).filter(|&s| s - i <= 10) {
                let name: String = chars[i + 1..semi].iter().collect();
                if let Some(decoded) = entity(&name) {
                    out.push_str(&decoded);
                    i = semi + 1;
                    continue;
                }
            }
        }
        // A line break in the source is a space, as in HTML; only the
        // breaking tags make lines.
        out.push(if c == '\n' || c == '\r' { ' ' } else { c });
        i += 1;
    }
    out.split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of an entity without its & and ;, or None if unknown.
fn entity(name: &str) -> Option<String> {
    let text = match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" | "#39" => "'",
        "nbsp" => " ",
        _ => {
            let code = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok()
            } else if let Some(dec) = name.strip_prefix('#') {
                dec.parse().ok()
            } else {
                None
            };
            return code.and_then(char::from_u32).map(String::from);
        }
    };
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anki::back_of_answer;
    use serde_json::Value;
    use std::path::Path;

    /// testdata/ankiconnect/card-text.json: cards recorded from the test
    /// Anki with the text the popup must show; the macOS tests read it too.
    #[test]
    fn recorded_cards_read_as_plain_text() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata/ankiconnect/card-text.json");
        let file: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let samples = file["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 7);
        for s in samples {
            let note = s["note"].as_str().unwrap();
            assert_eq!(plain(s["question"].as_str().unwrap()), s["front"].as_str().unwrap(), "{note}: front");
            assert_eq!(
                plain(back_of_answer(s["answer"].as_str().unwrap())),
                s["back"].as_str().unwrap(),
                "{note}: back"
            );
        }
    }

    #[test]
    fn style_and_script_blocks_are_dropped() {
        assert_eq!(plain("<style>.card { color: red }</style>word"), "word");
        assert_eq!(plain("<STYLE type=\"text/css\">x</STYLE>a<script>alert(1)</script>b"), "ab");
        assert_eq!(plain("<style>never closed"), "");
    }

    #[test]
    fn breaking_tags_make_lines_and_empty_lines_go() {
        assert_eq!(plain("a<br>b<br/>c<br />d"), "a\nb\nc\nd");
        assert_eq!(plain("a<div>b</div><div>c</div>"), "a\nb\nc");
        assert_eq!(plain("<p>a</p><p></p><p>b</p>"), "a\nb");
        assert_eq!(plain("<ul><li>one</li><li>two</li></ul>"), "one\ntwo");
        assert_eq!(plain("a<br><br><br>b"), "a\nb");
    }

    #[test]
    fn other_tags_and_images_are_dropped() {
        assert_eq!(plain("<b>bold</b> <i>it</i> <span style=\"x\">s</span>"), "bold it s");
        assert_eq!(plain("x<img src=\"a.png\">y"), "xy");
        assert_eq!(plain("<a href=\"https://example.com\">link</a>"), "link");
    }

    #[test]
    fn sound_references_are_dropped() {
        assert_eq!(plain("word [sound:word.mp3]"), "word");
        assert_eq!(plain("[anki:play:q:0]word[anki:play:a:1]"), "word");
        assert_eq!(plain("[not a sound] stays"), "[not a sound] stays");
    }

    #[test]
    fn entities_are_decoded() {
        assert_eq!(plain("&quot;q&quot; &amp; &lt;t&gt; &apos;a&#39;"), "\"q\" & <t> 'a'");
        assert_eq!(plain("caf&#233; &#x2014; na&iuml;ve"), "café — na&iuml;ve");
        assert_eq!(plain("a&nbsp;&nbsp;b"), "a b");
        assert_eq!(plain("AT&T and 5 < 6"), "AT&T and 5 < 6");
    }

    #[test]
    fn spaces_collapse_and_source_newlines_are_spaces() {
        assert_eq!(plain("  a   b \n c  "), "a b c");
        assert_eq!(plain("\n\n<hr id=answer>\n\nback"), "back");
    }
}
