//! The deliberately small, non-destructive frontmatter subset.
use std::ops::Range;
/// Recognized metadata and the original opaque span.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    /// Optional display title.
    pub title: Option<String>,
    /// Alternate link names.
    pub aliases: Vec<String>,
    /// Tag spellings before folding.
    pub tags: Vec<String>,
    /// Complete metadata byte span.
    pub span: Range<usize>,
    /// Number of properties, including opaque ones.
    pub properties: usize,
}
fn scalar(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_owned()
    } else {
        s.to_owned()
    }
}
fn values(s: &str) -> Vec<String> {
    let s = s.trim();
    if let Some(s) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let mut quote = None;
        let mut start = 0;
        let mut out = Vec::new();
        for (i, c) in s.char_indices() {
            if matches!(c, '\'' | '"') {
                if quote == Some(c) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(c);
                }
            } else if c == ',' && quote.is_none() {
                out.push(scalar(&s[start..i]));
                start = i + 1;
            }
        }
        out.push(scalar(&s[start..]));
        out.retain(|s| !s.is_empty());
        out
    } else if s.is_empty() {
        vec![]
    } else {
        vec![scalar(s)]
    }
}
/// Parse only a leading delimited block, ignoring unknown properties verbatim.
pub fn parse(text: &str) -> Option<Frontmatter> {
    let start = usize::from(text.starts_with('\u{feff}')) * 3;
    let mut lines = text.get(start..)?.split_inclusive('\n');
    if lines.next()?.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let mut out = Frontmatter::default();
    let mut offset = start + text[start..].find('\n')? + 1;
    let mut key = String::new();
    for line in lines {
        let clean = line.trim_end_matches(['\r', '\n']);
        offset += line.len();
        if clean == "---" || clean == "..." {
            out.span = start..offset;
            return Some(out);
        }
        let trimmed = clean.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("- ") {
            match key.as_str() {
                "aliases" => out.aliases.push(scalar(value)),
                "tags" => out.tags.push(scalar(value)),
                _ => {}
            }
            continue;
        }
        key.clear();
        if let Some((k, v)) = clean.split_once(':') {
            if k.starts_with(char::is_whitespace) {
                continue;
            }
            out.properties += 1;
            key.push_str(k.trim());
            let vals = values(v);
            match key.as_str() {
                "title" => out.title = vals.first().cloned(),
                "aliases" => out.aliases = vals,
                "tags" => out.tags = vals,
                _ => {}
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_quotes_remain_opaque() {
        for quote in ["\"", "'"] {
            let text = format!("---\ntitle: {quote}\naliases: [{quote}]\n---\n");
            let frontmatter = parse(&text).unwrap();
            assert_eq!(frontmatter.title.as_deref(), Some(quote));
            assert_eq!(frontmatter.aliases, [quote]);
        }
    }

    #[test]
    fn subset() {
        let f=parse("\u{feff}---\r\ntitle: 'Hi'\r\naliases: [one, \"two, three\"]\r\ntags:\r\n - A\r\n - B/C\r\nopaque: {x: 2}\r\n---\r\nBody").unwrap();
        assert_eq!(f.title.as_deref(), Some("Hi"));
        assert_eq!(f.aliases, ["one", "two, three"]);
        assert_eq!(f.tags, ["A", "B/C"]);
        assert_eq!(f.properties, 4);
        assert!(parse("---\nno end").is_none());
        assert_eq!(parse("---\n---\n").unwrap().properties, 0);
        assert!(parse("text\n---\n---").is_none());
    }
}
