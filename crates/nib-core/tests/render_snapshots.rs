//! Readable operation snapshots and embedding depth assertions.
use nib_core::{
    LinkTarget, NoteId,
    render::{self, RenderCtx},
};
struct Context;
impl RenderCtx for Context {
    fn resolve(&self, raw: &str) -> LinkTarget {
        if raw.starts_with("https:") {
            LinkTarget::External(raw.into())
        } else if raw.ends_with("png") {
            LinkTarget::Attachment(raw.into())
        } else if raw.starts_with("Note") {
            LinkTarget::Note {
                id: NoteId::new("Note").unwrap(),
                heading: None,
                block: None,
            }
        } else {
            LinkTarget::Unresolved(raw.into())
        }
    }
    fn embed(&self, _: &str) -> Option<String> {
        Some("# Inner\n![[Note]]\n".into())
    }
}
#[test]
fn elements() {
    for (name, input) in [
        (
            "inline",
            "# Heading\n## Sub\n*em* **strong** ~~strike~~ `code` [[Note|alias]] [[Missing]] [web](https://example.com)\n",
        ),
        (
            "blocks",
            "---\ntitle: Hi\ntags: [tag]\nopaque: yes\n---\n> [!note] Title\n> Quote\n\n- first\n  - second\n    - third\n\n1. ordered\n2. next\n\n- [ ] todo\n- [x] done\n\n---\n\n```rust\nfn main() {}\n```\n",
        ),
        (
            "table-footnotes",
            "| A | Longer |\n|---|---|\n| abc | z |\n\nFootnote[^a].\n\n[^a]: definition\n",
        ),
        (
            "embeds",
            "![[image.png|300]]\n![[Note]]\n![[Note#Inner]]\n![remote](https://example.com/x.png)\n",
        ),
    ] {
        let ops = render::render(input, &Context);
        let result = render::snapshot(&ops);
        let owned: Vec<_> = ops.into_iter().map(render::Op::into_owned).collect();
        assert_eq!(render::snapshot(&owned), result);
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/snapshots/{name}.txt"));
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&file, &result).unwrap();
        }
        assert_eq!(std::fs::read_to_string(file).unwrap(), result, "{name}");
    }
}
#[test]
fn section() {
    let ops = render::render_section("# One\na\n## Child\nb\n# Two\nc\n", "one", &Context);
    let s = render::snapshot(&ops);
    assert!(s.contains("Child"));
    assert!(!s.contains("Two"));
    assert!(render::render_section("# X", "missing", &Context).is_empty());
}
