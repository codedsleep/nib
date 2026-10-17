//! Code highlighting in the reading view. `GtkSourceView` tokenises a fenced
//! block in a scratch buffer, and each token's look is matched back to the
//! scheme style that produced it. The reading view's `syntax-*` tags take
//! those styles' colours in `reading::restyle`, so a scheme change recolours
//! highlighted code like the rest of the view.
use crate::gtk;
use sourceview5::prelude::*;
use std::{cell::RefCell, collections::HashMap};

/// Scheme styles a token can end up with, and the reading tag for each. A
/// language's own styles map onto these (`GtkSourceView`'s `def.lang`); when
/// two look the same in a scheme, the first listed names the token.
pub const STYLES: [(&str, &str); 26] = [
    ("def:comment", "syntax-comment"),
    ("def:keyword", "syntax-keyword"),
    ("def:type", "syntax-type"),
    ("def:string", "syntax-string"),
    ("def:number", "syntax-number"),
    ("def:constant", "syntax-constant"),
    ("def:identifier", "syntax-identifier"),
    ("def:preprocessor", "syntax-preprocessor"),
    ("def:special-char", "syntax-special-char"),
    ("def:statement", "syntax-statement"),
    ("def:function", "syntax-function"),
    ("def:builtin", "syntax-builtin"),
    ("def:boolean", "syntax-boolean"),
    ("def:special-constant", "syntax-special-constant"),
    ("def:character", "syntax-character"),
    ("def:floating-point", "syntax-floating-point"),
    ("def:decimal", "syntax-decimal"),
    ("def:base-n-integer", "syntax-base-n-integer"),
    ("def:operator", "syntax-operator"),
    ("def:doc-comment", "syntax-doc-comment"),
    ("def:shebang", "syntax-shebang"),
    ("def:note", "syntax-note"),
    ("def:error", "syntax-error"),
    ("def:net-address", "syntax-net-address"),
    ("def:insertion", "syntax-insertion"),
    ("def:deletion", "syntax-deletion"),
];

/// The whole of a highlighted block, in the scheme's text colour (the block
/// is otherwise one string colour); tokens are tagged over it.
pub const PLAIN: &str = "syntax-plain";

/// Blocks longer than this stay one colour.
const MAX_BYTES: usize = 256 * 1024;

/// What a tag shows: colour, bold, italic.
#[derive(Clone, PartialEq, Eq)]
struct Look {
    foreground: Option<String>,
    bold: bool,
    italic: bool,
}

fn look(tag: &gtk::TextTag) -> Option<Look> {
    let foreground = tag
        .is_foreground_set()
        .then(|| tag.foreground_rgba().map(|c| c.to_str().to_string()))
        .flatten();
    let bold = tag.is_weight_set() && tag.weight() >= 600;
    let italic = tag.is_style_set() && tag.style() == gtk::pango::Style::Italic;
    (foreground.is_some() || bold || italic).then_some(Look {
        foreground,
        bold,
        italic,
    })
}

thread_local! {
    static SCHEME: RefCell<Option<sourceview5::StyleScheme>> = const { RefCell::new(None) };
    static LOOKS: RefCell<Vec<(Look, &'static str)>> = const { RefCell::new(Vec::new()) };
    /// One scratch buffer per language: switching a buffer's language rebuilds
    /// its highlighting engine.
    static BUFFERS: RefCell<HashMap<&'static str, sourceview5::Buffer>> =
        RefCell::new(HashMap::new());
}

/// Follow the reading view's scheme (`reading::restyle`).
pub fn set_scheme(scheme: Option<&sourceview5::StyleScheme>) {
    let mut looks: Vec<(Look, &'static str)> = Vec::new();
    for (style, name) in STYLES {
        let Some(style) = scheme.and_then(|s| s.style(style)) else {
            continue;
        };
        let tag = gtk::TextTag::new(None);
        style.apply(&tag);
        if let Some(l) = look(&tag)
            && !looks.iter().any(|(seen, _)| *seen == l)
        {
            looks.push((l, name));
        }
    }
    LOOKS.with(|l| *l.borrow_mut() = looks);
    SCHEME.with(|s| s.borrow_mut().clone_from(&scheme.cloned()));
    BUFFERS.with(|b| {
        for buffer in b.borrow().values() {
            buffer.set_style_scheme(scheme);
        }
    });
}

/// Token spans of a fenced block, in characters from its start, or None when
/// the info string names no language nib highlights.
pub fn tokens(info: &str, code: &str) -> Option<Vec<(i32, i32, &'static str)>> {
    let id = nib_core::edit::fence_language(info)?;
    if code.len() > MAX_BYTES {
        return None;
    }
    let buffer = BUFFERS.with(|b| {
        if let Some(buffer) = b.borrow().get(id) {
            return Some(buffer.clone());
        }
        let language = sourceview5::LanguageManager::default().language(id)?;
        let buffer = sourceview5::Buffer::with_language(&language);
        buffer.set_style_scheme(SCHEME.with(|s| s.borrow().clone()).as_ref());
        b.borrow_mut().insert(id, buffer.clone());
        Some(buffer)
    })?;
    buffer.set_text(code);
    let (start, end) = buffer.bounds();
    buffer.ensure_highlight(&start, &end);
    let mut spans: Vec<(i32, i32, &'static str)> = Vec::new();
    LOOKS.with(|looks| {
        let looks = looks.borrow();
        let mut at = start;
        while at < end {
            let mut next = at;
            if !next.forward_to_tag_toggle(None::<&gtk::TextTag>) {
                next = end;
            }
            let shown = at
                .tags()
                .into_iter()
                .filter_map(|t| look(&t).map(|l| (t.priority(), l)))
                .max_by_key(|(priority, _)| *priority);
            let name = shown.and_then(|(_, l)| {
                looks
                    .iter()
                    .find(|(seen, _)| *seen == l)
                    .map(|(_, name)| *name)
            });
            if let Some(name) = name {
                match spans.last_mut() {
                    Some(last) if last.1 == at.offset() && last.2 == name => last.1 = next.offset(),
                    _ => spans.push((at.offset(), next.offset(), name)),
                }
            }
            at = next;
        }
    });
    buffer.set_text("");
    Some(spans)
}
