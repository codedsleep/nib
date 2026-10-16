//! Resolution and rewrites on realistic source spellings.
use nib_core::{Index, LinkTarget, NoteId, links, parse_note, rename_plan, settings::LinkFormat};
use std::time::SystemTime;
fn id(s: &str) -> NoteId {
    NoteId::new(s).unwrap()
}
fn m(s: &str, t: &str) -> nib_core::NoteMeta {
    parse_note(id(s), t, SystemTime::UNIX_EPOCH, t.len() as u64)
}
#[test]
fn variants_and_folder_moves() {
    let source =
        "[[hub]] [[hub#Heading|a]] ![[hub#^block]] [text](../hub.md) [angle](<../hub.md#Heading>)";
    let i = Index::build(vec![
        m("hub", "# Heading\ntext ^block"),
        m("folder/src", source),
    ]);
    assert_eq!(i.backlinks(&id("hub")).len(), 1);
    let plan = rename_plan(&i, &id("hub"), &id("new hub"));
    assert_eq!(plan.len(), 5);
    let mut edited = source.to_owned();
    for e in plan {
        edited.replace_range(e.byte_range, &e.replacement);
    }
    assert!(edited.contains("|a]]"));
    assert!(edited.contains("![[new hub#^block]]"));
    let after = Index::build(vec![m("new hub", "# Heading"), m("folder/src", &edited)]);
    assert!(after.unresolved.is_empty(), "{:?}", after.unresolved);
    let source = "[inner](other.md) [[folder/other]]";
    let i = Index::build(vec![m("folder/src", source), m("folder/other", "")]);
    let p = rename_plan(&i, &id("folder"), &id("moved"));
    assert_eq!(p.len(), 2);
    assert!(p.iter().all(|e| e.note == id("moved/src")));
    assert_eq!(links::relative_path("a/b", "c/d.md"), "../../c/d.md");
}
#[test]
fn writing_and_decoding() {
    let i = Index::build(vec![m("a/unique", ""), m("b/same", ""), m("a/same", "")]);
    assert_eq!(
        links::shortest_link(&i, &id("a/unique"), LinkFormat::Shortest),
        "unique"
    );
    assert_eq!(
        links::shortest_link(&i, &id("a/same"), LinkFormat::Shortest),
        "a/same"
    );
    assert_eq!(
        links::link_from(&i, &id("b/source"), &id("a/unique"), LinkFormat::Relative),
        "../a/unique"
    );
    assert_eq!(
        links::link_text("a", Some("Head"), None, Some("label"), true),
        "![[a#Head|label]]"
    );
    assert_eq!(links::decode_path("Some%20Note%é"), "Some Note%é");
    assert!(matches!(
        links::resolve(&i, &id("a/source"), "https://x"),
        LinkTarget::External(_)
    ));
    assert!(
        matches!(links::resolve_markdown(&i,&id("a/source"),"../b/same.md"),LinkTarget::Note{id:n,..}if n==id("b/same"))
    );
}
#[test]
fn fourteen_linkers() {
    let mut notes = vec![m("hub", "# Heading\ntext ^block")];
    for n in 0..14 {
        notes.push(m(
            &format!("linker{n}"),
            &format!("![[hub#Heading|alias {n}]]"),
        ));
    }
    let i = Index::build(notes);
    let p = rename_plan(&i, &id("hub"), &id("renamed"));
    assert_eq!(p.len(), 14);
    assert_eq!(i.backlinks(&id("hub")).len(), 14);
}
#[test]
fn dotted_relative_case_and_attachment_resolution() {
    let mut i = Index::build_with_attachments(
        vec![
            m("a/Plan 1.2", ""),
            m("a/CASE", ""),
            m("a/case", ""),
            m("b/source", "![[image.png]]"),
        ],
        vec!["attachments/image.png".into()],
    );
    for raw in ["Plan 1.2", "a/PLAN 1.2.MD", "../a/Plan 1.2"] {
        assert!(
            matches!(links::resolve(&i,&id("b/source"),raw),LinkTarget::Note{id:n,..}if n==id("a/Plan 1.2")),
            "{raw}"
        );
    }
    assert!(
        matches!(links::resolve(&i,&id("b/source"),"image.png"),LinkTarget::Attachment(p)if p=="attachments/image.png")
    );
    i.remove(&id("a/CASE"));
    assert!(
        matches!(links::resolve(&i,&id("b/source"),"A/CASE"),LinkTarget::Note{id:n,..}if n==id("a/case"))
    );
    i.upsert_attachment("b/image.png");
    assert!(
        matches!(&i.notes[&id("b/source")].links[0].target,LinkTarget::Attachment(p)if p=="b/image.png")
    );
    i.remove_attachment("b/image.png");
    assert!(
        matches!(&i.notes[&id("b/source")].links[0].target,LinkTarget::Attachment(p)if p=="attachments/image.png")
    );
    assert!(matches!(
        links::resolve(&i, &id("b/source"), "../../outside.png"),
        LinkTarget::Unresolved(_)
    ));
}
#[test]
fn reference_destinations_and_escape_spans() {
    let text = "[First][ref] and [Second][ref]\n\n[ref]: <hub.md#Heading> \"title\"\n";
    let i = Index::build(vec![m("hub", "# Heading"), m("source", text)]);
    let p = rename_plan(&i, &id("hub"), &id("renamed"));
    assert_eq!(p.len(), 1, "one definition destination is edited once");
    assert_eq!(p[0].expected, "hub.md#Heading");
    let mut next = text.into();
    for e in p {
        String::replace_range(&mut next, e.byte_range, &e.replacement);
    }
    assert!(next.contains("[First][ref]"));
    assert!(next.contains("[ref]: <renamed.md#Heading> \"title\""));
    let escaped = "[a](hub\\(x\\).md)";
    let meta = m("source", escaped);
    assert_eq!(meta.links[0].written, "hub\\(x\\).md");
    assert_eq!(&escaped[meta.links[0].byte_range.clone()], "hub\\(x\\).md");
}
#[test]
fn incremental_tables_match_rebuild_after_shared_backlink_removals() {
    let mut i = Index::build(
        (0..24)
            .map(|n| m(&format!("n{n}"), "[[n0]]\n[[n1]] #shared\n[[n0]]"))
            .collect(),
    );
    let initial = i.clone();
    i.upsert(m("extra", "[[n0]] #shared"));
    i.remove(&id("extra"));
    assert_eq!(i, initial);
    for n in 0..80 {
        let source = format!("n{}", n % 24);
        i.upsert(m(
            &source,
            &format!("[[n{}]]\n[[n{}]] #shared", (n + 2) % 24, (n + 3) % 24),
        ));
        assert_eq!(i, Index::build(i.notes.values().cloned().collect()));
    }
}
