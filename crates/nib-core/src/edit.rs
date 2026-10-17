//! Byte-offset editor behaviors, independent of vim mode and widgets.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
/// A single buffer edit and desired cursor offset after applying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Replaced byte range.
    pub range: Range<usize>,
    /// Inserted text.
    pub replacement: String,
    /// Cursor byte offset in the resulting buffer.
    pub cursor: usize,
}
fn edit(range: Range<usize>, replacement: String) -> TextEdit {
    let cursor = range.start + replacement.len();
    TextEdit {
        range,
        replacement,
        cursor,
    }
}
fn line(text: &str, offset: usize) -> Option<(usize, &str)> {
    if !text.is_char_boundary(offset) {
        return None;
    }
    let start = text[..offset].rfind(['\n', '\r']).map_or(0, |i| i + 1);
    let end = text[offset..]
        .find(['\n', '\r'])
        .map_or(text.len(), |i| offset + i);
    Some((start, &text[start..end]))
}
fn marker(line: &str) -> Option<(usize, usize, String)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let s = &line[indent..];
    let (mut end, next) = if s.starts_with("- ") || s.starts_with("* ") || s.starts_with("+ ") {
        (2, s[..2].to_owned())
    } else {
        let n = s.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 || !s[n..].starts_with(". ") {
            return None;
        }
        (
            n + 2,
            format!("{}. ", s[..n].parse::<u64>().ok()?.saturating_add(1)),
        )
    };
    let next = if s[end..].starts_with("[ ] ")
        || s[end..].starts_with("[x] ")
        || s[end..].starts_with("[X] ")
    {
        end += 4;
        format!("{next}[ ] ")
    } else {
        next
    };
    Some((indent, indent + end, next))
}
/// Continue a list at the cursor, or remove an empty list marker.
pub fn continue_list(text: &str, offset: usize) -> Option<TextEdit> {
    let (start, l) = line(text, offset)?;
    let (indent, end, next) = marker(l)?;
    if l[end..].trim().is_empty() {
        return Some(edit(start + indent..start + l.len(), String::new()));
    }
    Some(edit(offset..offset, format!("\n{}{next}", &l[..indent])))
}
/// Flip a task, or turn a plain list item into an unchecked task.
pub fn toggle_task(text: &str, offset: usize) -> Option<TextEdit> {
    let (start, l) = line(text, offset)?;
    let (_, end, _) = marker(l)?;
    let prefix = &l[..end];
    if prefix.ends_with("] ") {
        let at = start + end - 3;
        let replacement = if &text[at..=at] == " " { "x" } else { " " };
        Some(edit(at..at + 1, replacement.into()))
    } else {
        Some(edit(start + end..start + end, "[ ] ".into()))
    }
}
/// Promote (`-1`) or demote (`1`) a heading, clamping at level six.
pub fn cycle_heading(text: &str, offset: usize, direction: i8) -> Option<TextEdit> {
    let (start, l) = line(text, offset)?;
    let n = l.bytes().take_while(|b| *b == b'#').count();
    if n > 6 || (n > 0 && !l[n..].starts_with(' ')) {
        return None;
    }
    let next = if direction < 0 {
        n.saturating_sub(1)
    } else {
        (n + 1).min(6)
    };
    let end = start + n + usize::from(n > 0);
    Some(edit(
        start..end,
        if next == 0 {
            String::new()
        } else {
            format!("{} ", "#".repeat(next))
        },
    ))
}
/// Insert an enabled pair after the opener was typed. The cursor stays inside.
pub fn auto_pair(text: &str, offset: usize, enabled: &[String]) -> Option<TextEdit> {
    let before = text.get(..offset)?;
    for open in enabled {
        if !before.ends_with(open) {
            continue;
        }
        let close = match open.as_str() {
            "[[" => "]]",
            "**" => "**",
            "`" => "`",
            "(" => ")",
            "[" => "]",
            "\"" => "\"",
            _ => continue,
        };
        if text.get(offset..).is_some_and(|s| s.starts_with(close)) {
            return None;
        }
        return Some(TextEdit {
            range: offset..offset,
            replacement: close.into(),
            cursor: offset,
        });
    }
    None
}
/// Completion category and borrowed query; no allocation on the miss path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    /// Note names and aliases.
    Note,
    /// Attachments followed by notes.
    Embed,
    /// Heading in a target note.
    Heading,
    /// Body tag.
    Tag,
}
/// Borrowed completion context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion<'a> {
    /// Candidate kind.
    pub kind: CompletionKind,
    /// Typed query.
    pub query: &'a str,
    /// Relative byte span of the typed part.
    pub range: Range<usize>,
    /// Target before a heading fragment.
    pub target: Option<&'a str>,
}
/// Detect bracket and boundary-tag completions on one line.
pub fn completion_trigger(line: &str, offset: usize) -> Option<Completion<'_>> {
    let before = line.get(..offset)?;
    if let Some(start) = before.rfind("[[") {
        let query = &before[start + 2..];
        if !query.contains("]]") {
            if let Some((target, q)) = query.split_once('#') {
                return Some(Completion {
                    kind: CompletionKind::Heading,
                    query: q,
                    range: offset - q.len()..offset,
                    target: Some(target),
                });
            }
            return Some(Completion {
                kind: if start > 0 && line.as_bytes()[start - 1] == b'!' {
                    CompletionKind::Embed
                } else {
                    CompletionKind::Note
                },
                query,
                range: start + 2..offset,
                target: None,
            });
        }
    }
    let start = before.rfind('#')?;
    if before[..start]
        .chars()
        .next_back()
        .is_some_and(|c| !c.is_whitespace())
    {
        return None;
    }
    let query = &before[start + 1..];
    if query.chars().any(char::is_whitespace) {
        return None;
    }
    Some(Completion {
        kind: CompletionKind::Tag,
        query,
        range: start + 1..offset,
        target: None,
    })
}
/// Full source span and destination at a cursor, including bare web URLs.
pub fn link_at(text: &str, offset: usize) -> Option<(Range<usize>, String)> {
    let meta = crate::parse_note(
        crate::NoteId::new("buffer").ok()?,
        text,
        std::time::SystemTime::UNIX_EPOCH,
        0,
    );
    if let Some(l) = meta.links.into_iter().find(|l| l.span.contains(&offset)) {
        return Some((l.span, l.raw));
    }
    let (start, l) = line(text, offset)?;
    let at = offset - start;
    let left = l[..at].rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let end = l[at..]
        .find(char::is_whitespace)
        .map_or(l.len(), |i| at + i);
    let word = l[left..end].trim_end_matches(['.', ',', ')', '>']);
    crate::launch::url_allowed(word).then(|| (start + left..start + left + word.len(), word.into()))
}
/// Detected indentation unit.
pub fn detect_indent(text: &str) -> &str {
    for l in text.lines() {
        if l.starts_with('\t') {
            return "\t";
        }
        let n = l.bytes().take_while(|b| *b == b' ').count();
        if n > 0 {
            return if n >= 4 { "    " } else { "  " };
        }
    }
    "\t"
}
/// Indent or dedent the current list item using the note's own convention.
pub fn indent_list(text: &str, offset: usize, dedent: bool) -> Option<TextEdit> {
    let (start, l) = line(text, offset)?;
    marker(l)?;
    let unit = detect_indent(text);
    if dedent {
        let n = if l.starts_with('\t') {
            1
        } else {
            l.bytes()
                .take(unit.len())
                .take_while(|b| *b == b' ')
                .count()
        };
        Some(edit(start..start + n, String::new()))
    } else {
        Some(edit(start..start, unit.into()))
    }
}
/// Count Unicode words in visible text, excluding fenced/indented code.
pub fn word_count(text: &str) -> usize {
    let mut code = false;
    pulldown_cmark::Parser::new_ext(text, crate::parse::options())
        .filter_map(|e| match e {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_)) => {
                code = true;
                None
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::CodeBlock) => {
                code = false;
                None
            }
            pulldown_cmark::Event::Text(t) if !code => Some(t.unicode_words().count()),
            _ => None,
        })
        .sum()
}
/// Whether a line opens a fenced code block that names its language
/// (```` ```rust ````, `~~~ {.py}`): the editor's cue to load code highlighting.
pub fn has_named_fence(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        let Some(mark) = line.chars().next().filter(|c| matches!(c, '`' | '~')) else {
            return false;
        };
        let fence = line.len() - line.trim_start_matches(mark).len();
        fence >= 3
            && line[fence..]
                .trim_start()
                .trim_start_matches(['{', '.'])
                .starts_with(|c: char| c.is_alphanumeric())
    })
}
/// Live heading outline from the current buffer.
pub fn outline(text: &str) -> Vec<crate::Heading> {
    crate::parse_note(
        crate::NoteId::new("buffer").expect("constant valid id"),
        text,
        std::time::SystemTime::UNIX_EPOCH,
        0,
    )
    .headings
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_fences() {
        for text in ["```rust\n", "intro\n  ~~~ py\n", "````{.sh}\n", "```C++\n"] {
            assert!(has_named_fence(text), "{text:?}");
        }
        for text in [
            "```\ncode\n```\n",
            "``rust``",
            "~~ x",
            "text ```rust",
            "```  \n",
        ] {
            assert!(!has_named_fence(text), "{text:?}");
        }
    }
    #[test]
    fn editing() {
        for (s, next) in [
            ("- one", "\n- "),
            ("  * two", "\n  * "),
            ("9. n", "\n10. "),
            ("- [x] task", "\n- [ ] "),
        ] {
            assert_eq!(continue_list(s, s.len()).unwrap().replacement, next);
        }
        assert_eq!(continue_list("  - ", 4).unwrap().range, 2..4);
        assert_eq!(toggle_task("- [ ] todo", 5).unwrap().replacement, "x");
        assert_eq!(toggle_task("- [x] done", 5).unwrap().replacement, " ");
        assert_eq!(toggle_task("- item", 0).unwrap().replacement, "[ ] ");
        assert_eq!(cycle_heading("# Hi", 0, -1).unwrap().replacement, "");
        assert_eq!(
            cycle_heading("###### Hi", 0, 1).unwrap().replacement,
            "###### "
        );
        assert_eq!(
            auto_pair("[[", 2, &["[[".into()]).unwrap().replacement,
            "]]"
        );
        assert!(continue_list("text", 0).is_none());
        assert_eq!(detect_indent("a\n  - b"), "  ");
        assert_eq!(word_count("hello 世界\n```\ncode hidden\n```"), 3);
    }
    #[test]
    fn completions() {
        for (s, k, q) in [
            ("[[abc", CompletionKind::Note, "abc"),
            ("![[im", CompletionKind::Embed, "im"),
            ("[[Note#He", CompletionKind::Heading, "He"),
            ("a #ta", CompletionKind::Tag, "ta"),
        ] {
            let c = completion_trigger(s, s.len()).unwrap();
            assert_eq!(c.kind, k);
            assert_eq!(c.query, q);
        }
        for s in ["text", "abc#no", "[[done]]", "#tag word"] {
            assert!(completion_trigger(s, s.len()).is_none());
        }
        assert_eq!(link_at("[[Foo|A]]", 3).unwrap().1, "Foo");
        assert_eq!(outline("# Title")[0].text, "Title");
    }
}
