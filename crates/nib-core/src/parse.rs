//! One `CommonMark` event pass for headings, links, tags and block references.
use crate::{Heading, Link, LinkKind, LinkTarget, NoteId, NoteMeta, Tag, frontmatter, model::slug};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag as MdTag, TagEnd};
use std::{ops::Range, time::SystemTime};
/// Shared parser flags for indexing and reading view.
pub fn options() -> Options {
    Options::ENABLE_WIKILINKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}
/// One-based line/byte-column coordinates, from precomputed newline starts.
pub fn location(starts: &[usize], offset: usize) -> (u32, u32) {
    let i = starts.partition_point(|p| *p <= offset).saturating_sub(1);
    (
        u32::try_from(i + 1).unwrap_or(u32::MAX),
        u32::try_from(offset - starts[i] + 1).unwrap_or(u32::MAX),
    )
}
/// Byte starts for source lines.
pub fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}
/// Locate the destination in a parser-provided link span.
pub fn destination_span(text: &str, span: &Range<usize>, raw: &str, wiki: bool) -> Range<usize> {
    let source = &text[span.clone()];
    let at = if wiki {
        source.find("[[").map(|i| i + 2)
    } else if let Some(i) = source.find("](") {
        Some(i + 2 + usize::from(source[i + 2..].starts_with('<')))
    } else {
        source.find(raw)
    };
    let start = span.start + at.unwrap_or(0);
    let end = if wiki {
        text[start..span.end]
            .find(['|', ']'])
            .map_or(span.end, |i| start + i)
    } else {
        start + raw.len()
    };
    start..end.min(span.end)
}
fn scan_tags(
    text: &str,
    tags: &mut Vec<Tag>,
    blocks: &mut Vec<(String, u32)>,
    base: usize,
    starts: &[usize],
) {
    for (i, c) in text.char_indices() {
        if !matches!(c, '#' | '^')
            || text[..i]
                .chars()
                .next_back()
                .is_some_and(|p| !p.is_whitespace())
        {
            continue;
        }
        let word: String = text[i + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
            .collect();
        if word.is_empty() {
            continue;
        }
        if c == '#' && !word.chars().all(|c| c.is_ascii_digit()) {
            tags.push(Tag::new(&word));
        } else if c == '^' {
            blocks.push((word, location(starts, base + i).0));
        }
    }
}
/// Extract metadata; note text is never retained in the index.
#[allow(clippy::too_many_lines)] // One parser dispatch owns extraction state.
pub fn parse_note(id: NoteId, text: &str, mtime: SystemTime, size: u64) -> NoteMeta {
    let fm = frontmatter::parse(text);
    let starts = line_starts(text);
    let title = fm
        .as_ref()
        .and_then(|f| f.title.clone())
        .unwrap_or_else(|| id.stem().into());
    let mut meta = NoteMeta {
        id,
        title,
        aliases: fm.as_ref().map_or_else(Vec::new, |f| f.aliases.clone()),
        tags: fm
            .as_ref()
            .map_or_else(Vec::new, |f| f.tags.iter().map(|t| Tag::new(t)).collect()),
        headings: vec![],
        links: vec![],
        blocks: vec![],
        frontmatter: fm.map(|f| f.span),
        mtime,
        size,
    };
    let mut heading: Option<(u8, String, u32)> = None;
    let mut code = false;
    let mut metadata = false;
    for (event, span) in
        Parser::new_ext(text.trim_start_matches('\u{feff}'), options()).into_offset_iter()
    {
        let shift = usize::from(text.starts_with('\u{feff}')) * 3;
        let span = span.start + shift..span.end + shift;
        match event {
            Event::Start(MdTag::MetadataBlock(_)) => metadata = true,
            Event::End(TagEnd::MetadataBlock(_)) => metadata = false,
            Event::Start(MdTag::CodeBlock(_)) => code = true,
            Event::End(TagEnd::CodeBlock) => code = false,
            Event::Start(MdTag::Heading { level, .. }) => {
                heading = Some((level as u8, String::new(), location(&starts, span.start).0));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text, line)) = heading.take() {
                    meta.headings.push(Heading {
                        level,
                        slug: slug(&text),
                        text,
                        line,
                    });
                }
            }
            Event::Text(t) if !metadata && !code => {
                if let Some((_, h, _)) = &mut heading {
                    h.push_str(&t);
                }
                scan_tags(&t, &mut meta.tags, &mut meta.blocks, span.start, &starts);
            }
            Event::Code(t) => {
                if let Some((_, h, _)) = &mut heading {
                    h.push_str(&t);
                }
            }
            Event::Start(
                MdTag::Link {
                    link_type,
                    dest_url,
                    ..
                }
                | MdTag::Image {
                    link_type,
                    dest_url,
                    ..
                },
            ) => {
                let wiki = matches!(link_type, LinkType::WikiLink { .. });
                let source = &text[span.clone()];
                let embed = source.starts_with('!');
                let byte_range = destination_span(text, &span, &dest_url, wiki);
                let raw = if wiki {
                    text[byte_range.clone()].to_owned()
                } else {
                    dest_url.to_string()
                };
                let alias = if wiki {
                    source
                        .split_once('|')
                        .map(|(_, a)| a.trim_end_matches(']').to_owned())
                } else {
                    None
                };
                let (line, col) = location(&starts, span.start);
                meta.links.push(Link {
                    target: LinkTarget::Unresolved(raw.clone()),
                    raw,
                    alias,
                    embed,
                    line,
                    col,
                    byte_range,
                    span,
                    kind: if wiki {
                        LinkKind::Wiki
                    } else {
                        LinkKind::Markdown
                    },
                });
            }
            _ => {}
        }
    }
    meta.tags.sort();
    meta.tags.dedup();
    meta
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragment() {
        let s = "---\naliases: [Alt]\ntags: [One]\n---\n# Héllo **world**\n[[Foo#Title|alias]] ![[img.png|300]] [x](<Some Note.md>) #two/nest ^block\n`#skip`\n```\n#skiptoo\n```\n";
        let m = parse_note(NoteId::new("a").unwrap(), s, SystemTime::UNIX_EPOCH, 0);
        assert_eq!(m.headings[0].slug, "héllo-world");
        assert_eq!(m.links.len(), 3);
        assert_eq!(&s[m.links[0].byte_range.clone()], "Foo#Title");
        assert_eq!(m.links[1].alias.as_deref(), Some("300"));
        assert!(m.links[1].embed);
        assert_eq!(m.links[2].raw, "Some Note.md");
        assert_eq!(m.tags, vec![Tag::new("one"), Tag::new("two/nest")]);
        assert_eq!(m.blocks, [("block".into(), 6)]);
    }
}
