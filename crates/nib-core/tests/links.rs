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
