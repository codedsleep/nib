//! Release performance proof on a generated fixture (never a live vault).
use nib_core::{NoteId, search::SearchSpec, settings::Config};
use nib_vault::{
    Guard, IndexEvent, SearchEvent, VaultEvent, VaultHandle, fsx,
    fuzzy::{self, Matcher},
    theme,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};
fn event<T>(
    rx: &async_channel::Receiver<VaultEvent>,
    mut select: impl FnMut(VaultEvent) -> Option<T>,
) -> Result<T, Box<dyn std::error::Error>> {
    while let Ok(e) = rx.recv_blocking() {
        if let Some(v) = select(e) {
            return Ok(v);
        }
    }
    Err("event stream closed".into())
}
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
fn settled(m: &Matcher, query: u64) -> Result<fuzzy::Snapshot, Box<dyn std::error::Error>> {
    let events = m.events();
    loop {
        let s = events.recv_blocking()?;
        if s.query == query && !s.running {
            return Ok(s);
        }
    }
}
#[allow(clippy::too_many_lines)] // Sequential measurements printed as a compact handoff report.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: trace FIXTURE_DIR")?,
    )
    .canonicalize()?;
    if !root.join(".fixture-marker").is_file() {
        return Err("trace accepts a generated nib fixture only".into());
    }
    let cfg = Config::default();
    let start = Instant::now();
    let (h, rx) = VaultHandle::open(&root, &cfg, Guard::from_env())?;
    println!("open={:.3}ms", ms(start.elapsed()));
    let start = Instant::now();
    h.start_index();
    let index = event(&rx, |e| {
        if let VaultEvent::Index(IndexEvent::Ready(i)) = e {
            Some(i)
        } else {
            None
        }
    })?;
    println!(
        "index_ready={:.3}ms notes={}",
        ms(start.elapsed()),
        index.notes.len()
    );
    assert_eq!(
        index.backlinks(&NoteId::new("hub")?).len(),
        14,
        "fixture hub must have exactly fourteen linking notes"
    );
    let note = format!(
        "# Title\n[[Other]] #tag\n{}",
        "words in a realistic paragraph. ".repeat(160)
    );
    let mut samples = Vec::new();
    for _ in 0..200 {
        let start = Instant::now();
        std::hint::black_box(nib_core::parse_note(
            NoteId::new("sample")?,
            &note,
            SystemTime::UNIX_EPOCH,
            note.len() as u64,
        ));
        samples.push(start.elapsed());
    }
    samples.sort();
    println!(
        "parse_note_p50={:.3}us parse_note_p99={:.3}us",
        samples[100].as_secs_f64() * 1e6,
        samples[198].as_secs_f64() * 1e6
    );
    let line = "a".repeat(2000);
    let start = Instant::now();
    for _ in 0..200 {
        std::hint::black_box(nib_core::edit::completion_trigger(&line, line.len()));
    }
    println!(
        "completion_trigger={:.3}us",
        start.elapsed().as_secs_f64() * 1e6 / 200.0
    );
    let start = Instant::now();
    let search = h.search(SearchSpec::parse("fixture"));
    let hits = event(&rx, |e| match e {
        VaultEvent::Search(SearchEvent::Batch { hits, .. }) => Some(hits),
        _ => None,
    })?;
    println!(
        "search_first_batch={:.3}ms hits={}",
        ms(start.elapsed()),
        hits.len()
    );
    search.cancel_and_wait()?;
    let big = h.read_note(&root.join("big.md"))?.text;
    let start = Instant::now();
    let ops = nib_core::render::render(&big, &nib_core::render::Unresolved);
    println!(
        "render_ops_big={:.3}ms ops={}",
        ms(start.elapsed()),
        ops.len()
    );
    drop(ops);
    let folder = tempfile::tempdir()?;
    let guard = Guard::confined(vec![folder.path().into()]);
    for n in 0..20000 {
        fsx::create(&guard, &folder.path().join(format!("entry{n:05}.md")), b"")?;
    }
    let (list, events) = VaultHandle::open(folder.path(), &cfg, guard)?;
    let start = Instant::now();
    list.list(folder.path());
    event(&events, |e| {
        matches!(e, VaultEvent::Listing { .. }).then_some(())
    })?;
    println!("list_20k={:.3}ms", ms(start.elapsed()));
    drop(list);
    let matcher = Matcher::new(
        nib_core::fuzzy::switcher_candidates(&index, &[])
            .into_iter()
            .take(3000),
    )?;
    settled(&matcher, 0)?;
    let start = Instant::now();
    let query = matcher.set_query("note299");
    let snap = settled(&matcher, query)?;
    println!(
        "matcher_first={:.3}ms matches={}",
        ms(start.elapsed()),
        snap.matched
    );
    if let Some(file) = std::env::var_os("NIB_TRACE_GTK_BASE") {
        let base = std::fs::read_to_string(Path::new(&file))?;
        let css = theme::theme_css_with_base("NordArc-Theme", &base).ok_or("theme unavailable")?;
        println!("theme_css_bytes={}", css.len());
    }
    Ok(())
}
