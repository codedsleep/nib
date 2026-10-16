//! Pure algorithm edge cases independent of filesystem state.
use nib_core::{edit::*, settings::*, sort::*, *};
use std::time::SystemTime;
fn id(s: &str) -> NoteId {
    NoteId::new(s).unwrap()
}
#[test]
fn editing_edges() {
    assert!(toggle_task("plain", 0).is_none());
    assert!(cycle_heading("####### no", 0, 1).is_none());
    assert!(cycle_heading("#hashtag", 0, 1).is_none());
    assert!(auto_pair("x", 1, &["[[".into()]).is_none());
    assert!(auto_pair("[[]]", 2, &["[[".into()]).is_none());
    for (op, cl) in [
        ("**", "**"),
        ("`", "`"),
        ("(", ")"),
        ("[", "]"),
        ("\"", "\""),
    ] {
        assert_eq!(
            auto_pair(op, op.len(), &[op.into()]).unwrap().replacement,
            cl
        );
    }
    assert!(auto_pair("?", 1, &["?".into()]).is_none());
    assert_eq!(
        link_at("a https://example.com.", 5).unwrap().1,
        "https://example.com"
    );
    assert_eq!(detect_indent("\t- x"), "\t");
    assert_eq!(detect_indent("    - x"), "    ");
    assert_eq!(detect_indent("plain"), "\t");
    assert_eq!(indent_list("  - x", 3, false).unwrap().replacement, "  ");
    assert_eq!(indent_list("  - x", 3, true).unwrap().range, 0..2);
    assert_eq!(indent_list("\t- x", 2, true).unwrap().range, 0..1);
    assert!(completion_trigger("é", 1).is_none());
    assert!(continue_list("é", 1).is_none());
    assert!(link_at("plain", 3).is_none());
    assert!(continue_list("no", 99).is_none());
}
#[test]
fn settings_and_cli() {
    let (c, w) = Config::parse(
        "[editor]\nauto_pair=[1]\nmax_width='wrong'\n[files]\nsort='modified'\nlinemode='modified'\n[links]\nformat='relative'\nrewrite_on_rename='always'\n[vault]\nlast='/tmp/v'\n",
    );
    assert_eq!(w.len(), 2);
    assert_eq!(c.files.sort, Sort::Modified);
    assert_eq!(c.links.format, LinkFormat::Relative);
    assert!(!Config::parse("editor=2").1.is_empty());
    assert!(!Config::parse("unknown=true").1.is_empty());
    for s in [
        "--profile bad-name",
        "--profile a --profile b",
        "--new a --new b",
        "--version --help",
    ] {
        assert!(cli::parse(s.split_whitespace().map(str::to_owned)).is_err());
    }
    assert_eq!(
        cli::parse(["--".into(), "--note.md".into()])
            .unwrap()
            .path
            .unwrap()
            .to_str(),
        Some("--note.md")
    );
}
#[test]
fn sort_and_fuzzy() {
    let row = |name: &str, dir| Entry {
        path: name.into(),
        name: name.into(),
        directory: dir,
        modified: SystemTime::UNIX_EPOCH,
        size: 1,
    };
    let mut rows = vec![
        row("n10.md", false),
        row("n2.md", false),
        row("folder", true),
    ];
    sort_entries(&mut rows, Sort::Name, false);
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["folder", "n2.md", "n10.md"]
    );
    sort_entries(&mut rows, Sort::Name, true);
    assert_eq!(rows[1].name, "n10.md");
    sort_entries(&mut rows, Sort::Modified, false);
    assert!(visible(&rows[1], false, true));
    assert!(!visible(&row(".hidden.md", false), false, false));
    assert!(!visible(&row("a.png", false), true, true));
    assert_eq!(natural("a", "aa"), std::cmp::Ordering::Less);
    assert_eq!(natural("same", "same"), std::cmp::Ordering::Equal);
    assert_eq!(attach::collision_name("file", 1), "file 1");
    assert_eq!(attach::collision_name("file.png", 0), "file.png");
    let mut i = Index::build(vec![parse_note(
        id("a"),
        "# H\n#tag",
        SystemTime::UNIX_EPOCH,
        0,
    )]);
    i.attachments.push("x.png".into());
    assert!(matches!(
        fuzzy::completion_candidates(CompletionKind::Embed, &i, "")[0].target,
        fuzzy::CandidateTarget::Attachment(_)
    ));
    assert_eq!(
        fuzzy::completion_candidates(CompletionKind::Tag, &i, "").len(),
        1
    );
    assert_eq!(
        fuzzy::completion_candidates(CompletionKind::Note, &i, "").len(),
        1
    );
    assert!(fuzzy::completion_candidates(CompletionKind::Heading, &i, "missing").is_empty());
    i.rename(&id("a"), id("b"));
    assert!(i.notes.contains_key(&id("b")));
    i.remove(&id("missing"));
}
#[test]
fn format_and_links() {
    assert_eq!(text::decode(b"\r").unwrap().1.eol, text::Eol::Cr);
    assert_eq!(links::normalize_relative("a/../b"), Some("b".into()));
    assert!(links::normalize_relative("../../b").is_none());
    assert!(matches!(
        links::resolve_markdown(&Index::default(), &id("a"), "../../evil.png"),
        LinkTarget::Unresolved(_)
    ));
    assert_eq!(links::decode_path("bad%FF"), "bad%FF");
    assert_eq!(
        links::fragments("a#Title"),
        ("a", Some("title".into()), None)
    );
    assert!(launch::within_vault(
        std::path::Path::new("/v"),
        std::path::Path::new("/v/n.md")
    ));
    assert!(!launch::url_allowed("example.com"));
    assert!(clip::encode_uri_list(&["relative".into()]).is_err());
    assert!(clip::decode_gnome_copied("bad\nfile:///tmp/a").is_err());
    assert!(clip::decode_uri_list("file:///tmp/%00").is_err());
}
#[test]
#[ignore = "release timing probe; run before phase handoff"]
fn timings() {
    let note = format!(
        "# Title\n[[Other]] #tag\n{}",
        "words in a realistic paragraph. ".repeat(160)
    );
    let mut samples = Vec::new();
    for _ in 0..200 {
        let start = std::time::Instant::now();
        std::hint::black_box(parse_note(
            id("sample"),
            &note,
            SystemTime::UNIX_EPOCH,
            note.len() as u64,
        ));
        samples.push(start.elapsed());
    }
    samples.sort();
    let parse = samples[100];
    let line = "a".repeat(2000);
    let mut times = vec![];
    for _ in 0..200 {
        let start = std::time::Instant::now();
        std::hint::black_box(completion_trigger(&line, line.len()));
        times.push(start.elapsed());
    }
    times.sort();
    eprintln!(
        "parse_note_p50={:.3}us completion_trigger_p50={:.3}us",
        parse.as_secs_f64() * 1e6,
        times[100].as_secs_f64() * 1e6
    );
    assert!(parse.as_micros() <= 60);
    assert!(times[100].as_micros() <= 30);
}
#[test]
#[ignore = "release rendering budget probe"]
fn render_timing() {
    let text = "Line **Markdown** and [[Note]] with #tag and `code`.\n".repeat(10000);
    let start = std::time::Instant::now();
    let ops = render::render(&text, &render::Unresolved);
    let elapsed = start.elapsed();
    std::hint::black_box(ops);
    eprintln!("render_ops_big={:.3}ms", elapsed.as_secs_f64() * 1000.0);
    assert!(elapsed.as_millis() <= 12);
}
