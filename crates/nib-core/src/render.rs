//! Reading-view operations from the same parser configuration as the index.
use crate::{
    LinkTarget, frontmatter,
    parse::{line_starts, location, options},
};
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Parser, Tag, TagEnd};
use std::{ops::Range, sync::Arc};
const INDENTS: [&str; 32] = [
    "indent-0",
    "indent-1",
    "indent-2",
    "indent-3",
    "indent-4",
    "indent-5",
    "indent-6",
    "indent-7",
    "indent-8",
    "indent-9",
    "indent-10",
    "indent-11",
    "indent-12",
    "indent-13",
    "indent-14",
    "indent-15",
    "indent-16",
    "indent-17",
    "indent-18",
    "indent-19",
    "indent-20",
    "indent-21",
    "indent-22",
    "indent-23",
    "indent-24",
    "indent-25",
    "indent-26",
    "indent-27",
    "indent-28",
    "indent-29",
    "indent-30",
    "indent-31",
];
/// Link resolution and optional embed content supplied without I/O.
pub trait RenderCtx {
    /// Resolve a destination relative to the currently rendered note.
    fn resolve(&self, raw: &str) -> LinkTarget;
    /// Supply already loaded note Markdown for a one-level embed.
    fn embed(&self, _raw: &str) -> Option<String> {
        None
    }
}
/// Default context used for previews and tests.
pub struct Unresolved;
impl RenderCtx for Unresolved {
    fn resolve(&self, raw: &str) -> LinkTarget {
        LinkTarget::Unresolved(raw.into())
    }
}
/// Compact semantic style set: text runs do not allocate a tag vector.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Styles(u64);
const STYLE_NAMES: [&str; 51] = [
    "em",
    "strong",
    "strike",
    "code",
    "quote",
    "callout-title",
    "codeblock",
    "code-language",
    "link",
    "unresolved-link",
    "table",
    "footnote",
    "footnote-definition",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "indent-0",
    "indent-1",
    "indent-2",
    "indent-3",
    "indent-4",
    "indent-5",
    "indent-6",
    "indent-7",
    "indent-8",
    "indent-9",
    "indent-10",
    "indent-11",
    "indent-12",
    "indent-13",
    "indent-14",
    "indent-15",
    "indent-16",
    "indent-17",
    "indent-18",
    "indent-19",
    "indent-20",
    "indent-21",
    "indent-22",
    "indent-23",
    "indent-24",
    "indent-25",
    "indent-26",
    "indent-27",
    "indent-28",
    "indent-29",
    "indent-30",
    "indent-31",
];
impl Styles {
    fn new(names: &[&str]) -> Self {
        Self(
            names
                .iter()
                .filter_map(|name| STYLE_NAMES.iter().position(|n| n == name))
                .fold(0, |bits, index| bits | (1 << index)),
        )
    }
    /// Iterate the semantic GTK tag names to apply to this run.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        STYLE_NAMES
            .iter()
            .enumerate()
            .filter(move |(i, _)| self.0 & (1 << i) != 0)
            .map(|(_, name)| *name)
    }
}
impl std::fmt::Debug for Styles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}
/// Non-text content to attach to a GTK text buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// Vault-local image. Remote URLs are rendered as text instead.
    Image {
        /// Relative path.
        path: String,
        /// Optional width hint.
        width: Option<u32>,
    },
    /// A resolved note embed with its already built child operations.
    Embed {
        /// Original destination.
        target: String,
        /// Child operations, depth limited to one.
        ops: Vec<Op>,
    },
    /// Clickable task glyph; source line is on the enclosing operation.
    Task(bool),
    /// Horizontal separator.
    Rule,
    /// Opaque frontmatter expander.
    Frontmatter {
        /// Original block.
        raw: String,
        /// Number of properties.
        properties: usize,
    },
}
/// One insertion or child anchor, carrying one-based source lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Styled text run.
    Text {
        /// Text to insert.
        text: String,
        /// Semantic tag names, outermost first.
        tags: Styles,
        /// One-based half-open line range.
        lines: Range<u32>,
        /// Click destination, if any.
        link: Option<Arc<LinkTarget>>,
    },
    /// Widget anchor.
    Anchor {
        /// Child kind.
        kind: Anchor,
        /// Source line range.
        lines: Range<u32>,
    },
    /// Block or soft break.
    Newline {
        /// Source line range.
        lines: Range<u32>,
    },
}
impl Op {
    /// Source range shared by all operation types.
    pub fn lines(&self) -> &Range<u32> {
        match self {
            Self::Text { lines, .. } | Self::Anchor { lines, .. } | Self::Newline { lines } => {
                lines
            }
        }
    }
}
fn push(
    out: &mut Vec<Op>,
    text: impl Into<String>,
    tags: &[&'static str],
    lines: Range<u32>,
    link: Option<Arc<LinkTarget>>,
) {
    let text = text.into();
    if !text.is_empty() {
        out.push(Op::Text {
            text,
            tags: Styles::new(tags),
            lines,
            link,
        });
    }
}
fn newline(out: &mut Vec<Op>, lines: Range<u32>) {
    if !matches!(out.last(), Some(Op::Newline { .. })) {
        out.push(Op::Newline { lines });
    }
}
/// Build reading operations with source ranges and no widget allocations.
pub fn render(md: &str, ctx: &dyn RenderCtx) -> Vec<Op> {
    render_depth(md, ctx, 0)
}
#[allow(clippy::too_many_lines)] // One exhaustive parser-event dispatch keeps rendering and source mapping together.
fn render_depth(md: &str, ctx: &dyn RenderCtx, depth: u8) -> Vec<Op> {
    let starts = line_starts(md);
    let mut out = Vec::with_capacity(starts.len().saturating_mul(8));
    let fm = frontmatter::parse(md);
    let mut tags = Vec::<&'static str>::new();
    let mut stack = Vec::<usize>::new();
    let mut destinations = Vec::new();
    let mut lists = Vec::<Option<u64>>::new();
    let mut skip = 0_u32;
    let mut metadata = false;
    let mut quote = false;
    let mut callout_end = 0;
    let mut footnote_start = None;
    let mut definitions = Vec::new();
    let mut footnotes = Vec::<String>::new();
    let mut table: Option<Vec<Vec<String>>> = None;
    let mut row = Vec::new();
    let mut cell = String::new();
    if let Some(f) = &fm {
        out.push(Op::Anchor {
            kind: Anchor::Frontmatter {
                raw: md[f.span.clone()].into(),
                properties: f.properties,
            },
            lines: 1..location(&starts, f.span.end).0 + 1,
        });
    }
    let mut source_line = 0;
    let shift = usize::from(md.starts_with('\u{feff}')) * 3;
    for (event, span) in
        Parser::new_ext(md.trim_start_matches('\u{feff}'), options()).into_offset_iter()
    {
        let span = span.start + shift..span.end + shift;
        if span.start < starts[source_line]
            || starts
                .get(source_line + 1)
                .is_some_and(|next| span.start >= *next)
        {
            source_line = starts
                .partition_point(|p| *p <= span.start)
                .saturating_sub(1);
        }
        let first = u32::try_from(source_line + 1).unwrap_or(u32::MAX);
        let last = if starts
            .get(source_line + 1)
            .is_none_or(|next| span.end <= *next)
        {
            first
        } else {
            location(&starts, span.end.saturating_sub(1)).0
        };
        let lines = first..last + 1;
        if matches!(event, Event::Start(Tag::MetadataBlock(_))) {
            metadata = true;
            continue;
        }
        if matches!(event, Event::End(TagEnd::MetadataBlock(_))) {
            metadata = false;
            continue;
        }
        if metadata {
            continue;
        }
        if skip > 0 {
            match event {
                Event::Start(_) => skip += 1,
                Event::End(_) => skip -= 1,
                _ => {}
            }
            continue;
        }
        if span.start < callout_end
            && matches!(
                event,
                Event::Text(_) | Event::Code(_) | Event::SoftBreak | Event::HardBreak
            )
        {
            continue;
        }
        match event {
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                ..
            }) => {
                let wiki = matches!(link_type, LinkType::WikiLink { .. });
                let source = &md[span.clone()];
                let hint = source
                    .split_once('|')
                    .and_then(|(_, h)| h.trim_end_matches(']').parse().ok());
                let target = ctx.resolve(&dest_url);
                match target {
                    LinkTarget::Attachment(ref path) if !crate::links::has_scheme(path) => out
                        .push(Op::Anchor {
                            kind: Anchor::Image {
                                path: path.clone(),
                                width: hint,
                            },
                            lines: lines.clone(),
                        }),
                    LinkTarget::Note { .. } if wiki && depth == 0 => {
                        if let Some(text) = ctx.embed(&dest_url) {
                            let child = if let Some((_, heading)) = dest_url.split_once('#') {
                                render_section_depth(
                                    &text,
                                    &crate::model::slug(heading),
                                    ctx,
                                    depth + 1,
                                )
                            } else {
                                render_depth(&text, ctx, depth + 1)
                            };
                            out.push(Op::Anchor {
                                kind: Anchor::Embed {
                                    target: dest_url.to_string(),
                                    ops: child,
                                },
                                lines: lines.clone(),
                            });
                        } else {
                            push(
                                &mut out,
                                dest_url.to_string(),
                                &["link"],
                                lines.clone(),
                                Some(Arc::new(target)),
                            );
                        }
                    }
                    _ => push(
                        &mut out,
                        dest_url.to_string(),
                        &["link"],
                        lines.clone(),
                        Some(Arc::new(target)),
                    ),
                }
                skip = 1;
            }
            Event::Start(tag) => {
                stack.push(tags.len());
                match tag {
                    Tag::Heading { level, .. } => {
                        tags.push(["h1", "h2", "h3", "h4", "h5", "h6"][level as usize - 1]);
                    }
                    Tag::Emphasis => tags.push("em"),
                    Tag::Strong => tags.push("strong"),
                    Tag::Strikethrough => tags.push("strike"),
                    Tag::BlockQuote(_) => {
                        quote = true;
                        tags.push("quote");
                        let first = md[span.clone()].lines().next().unwrap_or_default();
                        if let Some(raw) = first
                            .trim_start_matches('>')
                            .trim_start()
                            .strip_prefix("[!")
                            && let Some((kind, title)) = raw.split_once(']')
                        {
                            push(
                                &mut out,
                                format!("{kind}{}", title.trim_end()),
                                &["quote", "callout-title"],
                                lines.clone(),
                                None,
                            );
                            out.push(Op::Newline {
                                lines: lines.clone(),
                            });
                            callout_end = span.start + first.len() + 1;
                        }
                    }
                    Tag::CodeBlock(kind) => {
                        tags.push("codeblock");
                        if let CodeBlockKind::Fenced(language) = kind
                            && !language.is_empty()
                        {
                            push(
                                &mut out,
                                format!("{language}\n"),
                                &["code-language"],
                                lines.clone(),
                                None,
                            );
                        }
                    }
                    Tag::List(n) => lists.push(n),
                    Tag::Item => {
                        let level = lists.len().saturating_sub(1);
                        tags.push(INDENTS[level.min(INDENTS.len() - 1)]);
                        let number = lists.last_mut().and_then(Option::as_mut).map(|n| {
                            let v = *n;
                            *n += 1;
                            v
                        });
                        let marker = number.map_or_else(
                            || format!("{} ", ["•", "◦", "▪"][level % 3]),
                            |n| format!("{n}. "),
                        );
                        push(&mut out, marker, &tags, lines.clone(), None);
                    }
                    Tag::Link { dest_url, .. } => {
                        let target = ctx.resolve(&dest_url);
                        tags.push(if matches!(target, LinkTarget::Unresolved(_)) {
                            "unresolved-link"
                        } else {
                            "link"
                        });
                        destinations.push(Arc::new(target));
                    }
                    Tag::FootnoteDefinition(name) => {
                        footnote_start = Some(out.len());
                        tags.push("footnote-definition");
                        push(&mut out, format!("[{name}] "), &tags, lines.clone(), None);
                    }
                    Tag::Table(_) => table = Some(Vec::new()),
                    Tag::TableHead | Tag::TableRow => row.clear(),
                    Tag::TableCell => cell.clear(),
                    _ => {}
                }
            }
            Event::End(end) => {
                match end {
                    TagEnd::Link => {
                        destinations.pop();
                    }
                    TagEnd::List(_) => {
                        lists.pop();
                    }
                    TagEnd::BlockQuote(_) => quote = false,
                    TagEnd::TableCell => row.push(std::mem::take(&mut cell)),
                    TagEnd::TableHead | TagEnd::TableRow => {
                        if let Some(t) = &mut table {
                            t.push(std::mem::take(&mut row));
                        }
                    }
                    TagEnd::Table => {
                        if let Some(t) = table.take() {
                            let widths: Vec<_> = (0..t.iter().map(Vec::len).max().unwrap_or(0))
                                .map(|c| {
                                    t.iter()
                                        .map(|r| r.get(c).map_or(0, |s| s.chars().count()))
                                        .max()
                                        .unwrap_or(0)
                                })
                                .collect();
                            for r in t {
                                let s = r
                                    .iter()
                                    .enumerate()
                                    .map(|(i, c)| {
                                        format!(
                                            "{}{}",
                                            c,
                                            " ".repeat(widths[i].saturating_sub(c.chars().count()))
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" │ ");
                                push(&mut out, s, &["table"], lines.clone(), None);
                                out.push(Op::Newline {
                                    lines: lines.clone(),
                                });
                            }
                        }
                    }
                    TagEnd::FootnoteDefinition => {
                        newline(&mut out, lines.clone());
                        if let Some(start) = footnote_start.take() {
                            definitions.extend(out.split_off(start));
                        }
                    }
                    TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock => {
                        newline(&mut out, lines.clone());
                    }
                    _ => {}
                }
                if let Some(n) = stack.pop() {
                    tags.truncate(n);
                }
            }
            Event::Text(t) => {
                if table.is_some() {
                    cell.push_str(&t);
                } else if quote && t.starts_with("[!") {
                    if let Some(end) = t.find(']') {
                        push(
                            &mut out,
                            format!("{}{}", &t[2..end], &t[end + 1..]),
                            &["quote", "callout-title"],
                            lines,
                            None,
                        );
                    } else {
                        push(
                            &mut out,
                            t.to_string(),
                            &tags,
                            lines,
                            destinations.last().cloned(),
                        );
                    }
                } else {
                    push(
                        &mut out,
                        t.to_string(),
                        &tags,
                        lines,
                        destinations.last().cloned(),
                    );
                }
            }
            Event::Code(t) => {
                if table.is_some() {
                    cell.push_str(&t);
                } else {
                    let mut code_tags = tags.clone();
                    code_tags.push("code");
                    push(
                        &mut out,
                        t.to_string(),
                        &code_tags,
                        lines,
                        destinations.last().cloned(),
                    );
                }
            }
            Event::SoftBreak | Event::HardBreak => out.push(Op::Newline { lines }),
            Event::Rule => out.push(Op::Anchor {
                kind: Anchor::Rule,
                lines,
            }),
            Event::TaskListMarker(done) => out.push(Op::Anchor {
                kind: Anchor::Task(done),
                lines,
            }),
            Event::FootnoteReference(name) => {
                let n = footnotes
                    .iter()
                    .position(|s| s == name.as_ref())
                    .unwrap_or_else(|| {
                        footnotes.push(name.to_string());
                        footnotes.len() - 1
                    })
                    + 1;
                push(&mut out, n.to_string(), &["footnote"], lines, None);
            }
            Event::Html(t) | Event::InlineHtml(t) => {
                push(&mut out, t.to_string(), &tags, lines, None);
            }
            _ => {}
        }
    }
    out.extend(definitions);
    out
}
/// Render one heading and its descendants, ending at the next peer heading.
pub fn render_section(md: &str, heading_slug: &str, ctx: &dyn RenderCtx) -> Vec<Op> {
    render_section_depth(md, heading_slug, ctx, 0)
}
fn render_section_depth(md: &str, slug: &str, ctx: &dyn RenderCtx, depth: u8) -> Vec<Op> {
    let headings = crate::edit::outline(md);
    let Some(h) = headings.iter().find(|h| h.slug == slug) else {
        return vec![];
    };
    let end = headings
        .iter()
        .find(|n| n.line > h.line && n.level <= h.level)
        .map_or(u32::MAX, |h| h.line);
    render_depth(md, ctx, depth)
        .into_iter()
        .filter(|op| op.lines().start >= h.line && op.lines().start < end)
        .collect()
}
/// Stable plain snapshot including anchors and semantic tags.
pub fn snapshot(ops: &[Op]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for op in ops {
        match op {
            Op::Text { text, tags, .. } => {
                let _ = writeln!(out, "TEXT {tags:?} {text:?}");
            }
            Op::Anchor { kind, .. } => {
                let _ = writeln!(out, "ANCHOR {kind:?}");
            }
            Op::Newline { .. } => out.push_str("NEWLINE\n"),
        }
    }
    out
}
