//! Prebuilt finder candidates and pure fzf query parsing.
use crate::{Index, LinkTarget, NoteId, edit::CompletionKind, links::resolve};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
/// Parsed fzf query; execution belongs to vault's matcher service.
pub struct Query {
    /// Original query.
    pub text: String,
    /// Nucleo's pure matching pattern.
    pub pattern: Pattern,
}
impl Query {
    /// Parse smart-case fzf operators.
    pub fn parse(text: &str) -> Self {
        Self {
            text: text.into(),
            pattern: Pattern::parse(text, CaseMatching::Smart, Normalization::Smart),
        }
    }
}
/// Destination represented by a finder row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateTarget {
    /// A note.
    Note(NoteId),
    /// A tag.
    Tag(String),
    /// A heading slug within a note.
    Heading(NoteId, String),
    /// Vault-relative attachment.
    Attachment(String),
}
/// Immutable candidate suitable for nucleo injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Visible text.
    pub label: String,
    /// Full matching text.
    pub text: String,
    /// Destination for acceptance.
    pub target: CandidateTarget,
}
/// Build title, alias and path candidates; empty-query order is recent-first.
pub fn switcher_candidates(index: &Index, recent: &[NoteId]) -> Vec<Candidate> {
    let mut notes: Vec<_> = index.notes.values().collect();
    notes.sort_by_key(|m| {
        (
            recent.iter().position(|n| n == &m.id).unwrap_or(usize::MAX),
            m.id.clone(),
        )
    });
    let mut out = Vec::new();
    for m in notes {
        out.push(Candidate {
            label: m.title.clone(),
            text: format!("{} {}", m.title, m.id),
            target: CandidateTarget::Note(m.id.clone()),
        });
        for a in &m.aliases {
            out.push(Candidate {
                label: format!("{a} → {}", m.title),
                text: format!("{a} {} {}", m.title, m.id),
                target: CandidateTarget::Note(m.id.clone()),
            });
        }
    }
    out
}
/// Alphabetically sorted tags with source counts.
pub fn tag_candidates(index: &Index) -> Vec<Candidate> {
    index
        .tags
        .iter()
        .map(|(t, ids)| Candidate {
            label: format!("#{} ({})", t.0, ids.len()),
            text: t.0.clone(),
            target: CandidateTarget::Tag(t.0.clone()),
        })
        .collect()
}
/// Build context-sensitive completion rows. Attachment paths are indexed separately
/// because they are never parsed as notes.
pub fn completion_candidates(kind: CompletionKind, index: &Index, target: &str) -> Vec<Candidate> {
    match kind {
        CompletionKind::Tag => tag_candidates(index),
        CompletionKind::Heading => {
            let from = NoteId::new("completion").expect("constant id");
            let LinkTarget::Note { id, .. } = resolve(index, &from, target) else {
                return vec![];
            };
            index.notes.get(&id).map_or_else(Vec::new, |m| {
                m.headings
                    .iter()
                    .map(|h| Candidate {
                        label: h.text.clone(),
                        text: h.text.clone(),
                        target: CandidateTarget::Heading(id.clone(), h.slug.clone()),
                    })
                    .collect()
            })
        }
        CompletionKind::Note => switcher_candidates(index, &[]),
        CompletionKind::Embed => {
            let mut c: Vec<_> = index
                .attachments
                .iter()
                .map(|p| Candidate {
                    label: p.clone(),
                    text: p.clone(),
                    target: CandidateTarget::Attachment(p.clone()),
                })
                .collect();
            c.extend(switcher_candidates(index, &[]));
            c
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidates() {
        let id = NoteId::new("a").unwrap();
        let i = Index::build(vec![crate::parse_note(
            id.clone(),
            "---\naliases: [Alias]\n---\n# Hi\n#tag",
            std::time::SystemTime::UNIX_EPOCH,
            0,
        )]);
        assert_eq!(switcher_candidates(&i, &[id]).len(), 2);
        assert!(switcher_candidates(&i, &[])[1].label.contains('→'));
        assert_eq!(tag_candidates(&i)[0].label, "#tag (1)");
        assert_eq!(
            completion_candidates(CompletionKind::Heading, &i, "a")[0].label,
            "Hi"
        );
        assert_eq!(Query::parse("^abc !x").pattern.atoms.len(), 2);
    }
}
