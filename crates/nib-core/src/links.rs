//! Obsidian resolution and byte-preserving note/folder rename plans.
use crate::{Edit, Index, LinkKind, LinkTarget, NoteId, settings::LinkFormat};
/// Split a destination into note path, optional heading and optional block.
pub fn fragments(raw: &str) -> (&str, Option<String>, Option<String>) {
    match raw.split_once('#') {
        Some((p, f)) if f.starts_with('^') => (p, None, Some(f[1..].into())),
        Some((p, f)) => (p, Some(crate::model::slug(f)), None),
        None => (raw, None, None),
    }
}
fn choose(ids: &std::collections::BTreeSet<NoteId>, from: &NoteId) -> Option<NoteId> {
    ids.iter()
        .min_by_key(|id| (id.folder() != from.folder(), id.as_str().len(), id.as_str()))
        .cloned()
}
/// Resolve a wikilink under the documented path, stem, alias and tie-break rules.
pub fn resolve(index: &Index, from: &NoteId, raw: &str) -> LinkTarget {
    let (path, heading, block) = fragments(raw);
    let path = decode_path(path);
    if has_scheme(&path) {
        return LinkTarget::External(raw.into());
    }
    let id = if path.is_empty() {
        Some(from.clone())
    } else if path.contains('/') {
        let clean = if path.starts_with("./") || path.starts_with("../") {
            format!("{}/{path}", from.folder())
        } else {
            path.trim_start_matches('/').to_owned()
        };
        NoteId::new(&clean).ok().and_then(|id| {
            index
                .notes
                .get(&id)
                .map(|m| m.id.clone())
                .or_else(|| index.by_path.get(&id.as_str().to_lowercase()).cloned())
        })
    } else {
        let stem = NoteId::new(&path)
            .map_or_else(|_| path.to_lowercase(), |id| id.as_str().to_lowercase());
        index
            .by_stem
            .get(&stem)
            .and_then(|ids| choose(ids, from))
            .or_else(|| index.by_alias.get(&stem).and_then(|ids| choose(ids, from)))
    };
    id.map_or_else(
        || {
            if is_attachment(&path) {
                attachment(index, from, &path).map_or_else(
                    || LinkTarget::Unresolved(raw.into()),
                    LinkTarget::Attachment,
                )
            } else {
                LinkTarget::Unresolved(raw.into())
            }
        },
        |id| LinkTarget::Note { id, heading, block },
    )
}
fn attachment(index: &Index, from: &NoteId, path: &str) -> Option<String> {
    let path = if path.starts_with("./") || path.starts_with("../") {
        format!("{}/{path}", from.folder())
    } else {
        path.into()
    };
    let path = normalize_relative(&path)?;
    let lower = path.to_lowercase();
    if path.contains('/') {
        return Some(
            index
                .attachments
                .iter()
                .find(|p| p.eq_ignore_ascii_case(&path))
                .cloned()
                .unwrap_or(path),
        );
    }
    Some(
        index
            .attachments
            .iter()
            .filter(|p| {
                p.rsplit('/')
                    .next()
                    .is_some_and(|name| name.to_lowercase() == lower)
            })
            .min_by_key(|p| {
                (
                    p.rsplit_once('/').map_or("", |(folder, _)| folder) != from.folder(),
                    p.len(),
                    p.as_str(),
                )
            })
            .cloned()
            .unwrap_or(path),
    )
}
/// Resolve `CommonMark` relative destinations from the linking note's folder.
pub fn resolve_markdown(index: &Index, from: &NoteId, raw: &str) -> LinkTarget {
    if has_scheme(raw) || raw.starts_with('#') {
        return resolve(index, from, raw);
    }
    let (path, heading, block) = fragments(raw);
    let decoded = decode_path(path);
    let joined = if decoded.starts_with('/') {
        decoded.trim_start_matches('/').to_owned()
    } else if from.folder().is_empty() {
        decoded
    } else {
        format!("{}/{decoded}", from.folder())
    };
    if is_attachment(&joined) {
        return normalize_relative(&joined).map_or_else(
            || LinkTarget::Unresolved(raw.into()),
            LinkTarget::Attachment,
        );
    }
    NoteId::new(&joined)
        .ok()
        .filter(|id| index.notes.contains_key(id))
        .map_or_else(
            || LinkTarget::Unresolved(raw.into()),
            |id| LinkTarget::Note { id, heading, block },
        )
}
/// Whether a path has a URL scheme, including disallowed launch schemes.
pub fn has_scheme(raw: &str) -> bool {
    raw.split_once(':').is_some_and(|(s, _)| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}
/// Non-Markdown extension denotes an attachment.
pub fn is_attachment(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .and_then(|s| s.rsplit_once('.'))
        .is_some_and(|(stem, e)| !stem.is_empty() && !e.eq_ignore_ascii_case("md"))
}
/// Normalize a relative attachment path without allowing root escapes.
pub fn normalize_relative(path: &str) -> Option<String> {
    let mut parts = vec![];
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(p),
        }
    }
    Some(parts.join("/"))
}
/// Decode percent-encoded UTF-8 in local Markdown paths.
pub fn decode_path(path: &str) -> String {
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < path.len() {
        if path.as_bytes()[i] == b'%'
            && i + 2 < path.len()
            && let (Some(a), Some(b)) = (
                char::from(path.as_bytes()[i + 1]).to_digit(16),
                char::from(path.as_bytes()[i + 2]).to_digit(16),
            )
        {
            bytes.push(u8::try_from(a * 16 + b).expect("two hex digits fit u8"));
            i += 3;
            continue;
        }
        bytes.push(path.as_bytes()[i]);
        i += 1;
    }
    String::from_utf8(bytes).unwrap_or_else(|_| path.into())
}
/// Emit a shortest unique stem or a vault-relative path.
/// Relative format without a source uses the root; use `link_from` when writing.
pub fn shortest_link(index: &Index, target: &NoteId, fmt: LinkFormat) -> String {
    if fmt == LinkFormat::Shortest
        && index
            .by_stem
            .get(&target.stem().to_lowercase())
            .is_some_and(|v| v.len() == 1)
    {
        target.stem().into()
    } else {
        target.to_string()
    }
}
/// Relative destination from a source folder to a target path.
pub fn relative_path(from: &str, to: &str) -> String {
    let a: Vec<_> = from.split('/').filter(|s| !s.is_empty()).collect();
    let b: Vec<_> = to.split('/').collect();
    let common = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    std::iter::repeat_n("..", a.len() - common)
        .chain(b[common..].iter().copied())
        .collect::<Vec<_>>()
        .join("/")
}
/// Write a link destination with a source-aware relative option.
pub fn link_from(index: &Index, from: &NoteId, target: &NoteId, fmt: LinkFormat) -> String {
    if fmt == LinkFormat::Relative {
        relative_path(from.folder(), target.as_str())
    } else {
        shortest_link(index, target, fmt)
    }
}
/// Build bracket syntax without losing alias, fragment or embed marker.
pub fn link_text(
    target: &str,
    heading: Option<&str>,
    block: Option<&str>,
    alias: Option<&str>,
    embed: bool,
) -> String {
    format!(
        "{}[[{target}{}{}]]",
        if embed { "!" } else { "" },
        block.map_or_else(
            || heading.map_or_else(String::new, |h| format!("#{h}")),
            |b| format!("#^{b}")
        ),
        alias.map_or_else(String::new, |a| format!("|{a}"))
    )
}
fn moved(id: &NoteId, old: &NoteId, new: &NoteId) -> NoteId {
    if id == old {
        new.clone()
    } else if let Some(tail) = id.as_str().strip_prefix(&format!("{old}/")) {
        NoteId::new(&format!("{new}/{tail}")).expect("renamed valid id stays valid")
    } else {
        id.clone()
    }
}
/// Plan changes for a note or a directory prefix. Apply after the move and remap
/// source IDs through the same rename when an edited source itself moved.
pub fn rename_plan(index: &Index, old: &NoteId, new: &NoteId) -> Vec<Edit> {
    let mut edits = Vec::new();
    for meta in index.notes.values() {
        let source = moved(&meta.id, old, new);
        for l in &meta.links {
            let LinkTarget::Note { id, .. } = &l.target else {
                continue;
            };
            let target = moved(id, old, new);
            if target == *id && (source == meta.id || l.kind != LinkKind::Markdown) {
                continue;
            }
            let (_, _, _) = fragments(&l.raw);
            let fragment = l.raw.split_once('#').map_or("", |(_, f)| f);
            let mut replacement = if l.kind == LinkKind::Markdown {
                relative_path(source.folder(), &target.filename()).replace(' ', "%20")
            } else {
                target.to_string()
            };
            if !fragment.is_empty() {
                replacement.push('#');
                replacement.push_str(fragment);
            }
            edits.push(Edit {
                note: source.clone(),
                byte_range: l.byte_range.clone(),
                replacement,
                expected: l.written.clone(),
                source_hash: meta.text_hash,
            });
        }
    }
    edits.sort_by(|a, b| {
        a.note
            .cmp(&b.note)
            .then_with(|| b.byte_range.start.cmp(&a.byte_range.start))
    });
    edits.dedup();
    edits
}
