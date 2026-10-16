//! Deterministic Obsidian-shaped test vault; never targets a live vault.
use nib_vault::{Guard, VaultError, fsx};
use std::fmt::Write as _;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4, 0,
    0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5, 1, 1,
    39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn put(g: &Guard, root: &Path, name: &str, bytes: &[u8]) -> Result<(), VaultError> {
    let path = root.join(name);
    if let Some(p) = path.parent() {
        fsx::create_dir_all(g, p)?;
    }
    fsx::save(g, &path, bytes)?;
    Ok(())
}
#[allow(clippy::too_many_lines)] // A linear, deterministic fixture recipe.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = 7_u64;
    let mut notes = 3000_usize;
    let mut root = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--seed" => seed = args.next().ok_or("missing seed")?.parse()?,
            "--notes" => notes = args.next().ok_or("missing count")?.parse()?,
            _ if !a.starts_with('-') && root.is_none() => root = Some(PathBuf::from(a)),
            _ => return Err(format!("unexpected argument {a}").into()),
        }
    }
    let root = root.unwrap_or_else(|| glib::user_cache_dir().join("nib-fixture"));
    let root = std::path::absolute(root)?;
    if root.join(".obsidian").exists() && !root.join(".fixture-marker").exists() {
        return Err("refused to generate over an existing vault".into());
    }
    let g = Guard::from_env();
    g.check(&root)?;
    fsx::create_dir_all(&g, &root)?;
    put(
        &g,
        &root,
        ".fixture-marker",
        format!("seed={seed}\nnotes={notes}\n").as_bytes(),
    )?;
    for i in 0..notes {
        let path = format!("folder{:02}/note{i:05}.md", i % 40);
        let target = (i + 1) % notes;
        let header = format!(
            "---\ntitle: Note {i}\naliases: [alias{i}]\ntags: [fixture, group/{}]\nopaque: keep me unchanged\n---\n# Note {i}\n\n[[folder{:02}/note{target:05}|next]] and [[missing-{i}]] #body/tag\n\n- [ ] task\n- [x] done\n\n> [!note] Remember\n> A fixture callout.\n\n## Detail\nText with **bold**, *emphasis*, `code` and [relative](../folder{:02}/note{target:05}.md). ^block-{i}\n\n![[attachments/image{:02}.png|300]]\n\n",
            i % 5,
            target % 40,
            target % 40,
            i % 20
        );
        // Approx. 40 MB across 3,000 files, with realistic paragraph/link density.
        let mut body = header;
        for j in 0..84 {
            let _ = write!(
                body,
                "Fixture paragraph {j}: deterministic seed {seed}, note {i}. Writing notes keeps ideas useful. This sentence is plain Markdown with unicode café and 世界.\n\n"
            );
        }
        put(&g, &root, &path, body.as_bytes())?;
    }
    for (n, b) in [
        ("crlf.md", b"# CRLF\r\nText\r\n".as_slice()),
        ("bom.md", b"\xef\xbb\xbf# BOM\n"),
        ("mixed-eol.md", b"# Mixed\r\nLine\nOther\rEnd"),
        ("no-trailing-newline.md", b"# No newline"),
        ("latin1.md", b"caf\xe9\n"),
        ("hub.md", b"# Hub\n## Heading\nHub paragraph. ^block\n"),
        ("ambiguous.md", b"# Root ambiguous\n"),
        ("folder00/ambiguous.md", b"# Folder ambiguous\n"),
        (
            "embeds.md",
            b"![[folder00/note00000]]\n![[folder00/note00000#Detail]]\n![[embeds]]\n",
        ),
    ] {
        put(&g, &root, n, b)?;
    }
    for i in 0..14 {
        put(
            &g,
            &root,
            &format!("linker{i:02}.md"),
            format!(
                "# Linker {i}\n{}[[hub{}|alias {i}]]\n",
                if i % 2 == 0 { "!" } else { "" },
                if i % 3 == 0 {
                    "#Heading"
                } else if i % 3 == 1 {
                    "#^block"
                } else {
                    ""
                }
            )
            .as_bytes(),
        )?;
    }
    let mut big = String::new();
    for i in 0..10000 {
        let _ = writeln!(
            big,
            "Line {i}: **Markdown** and [[ambiguous]] with #large and `code`."
        );
    }
    put(&g, &root, "big.md", big.as_bytes())?;
    for d in 1..=30 {
        put(
            &g,
            &root,
            &format!("daily/2026-09-{d:02}.md"),
            format!("# 2026-09-{d:02}\nDaily fixture.\n").as_bytes(),
        )?;
    }
    for n in ["daily", "meeting", "note"] {
        put(
            &g,
            &root,
            &format!("templates/{n}.md"),
            b"# {{title}}\n{{date}} {{time}}\n",
        )?;
    }
    // Valid, tiny PNG. UI image decoding never requires network access.
    for i in 0..20 {
        put(&g, &root, &format!("attachments/image{i:02}.png"), PNG)?;
    }
    put(&g, &root, "attachments/evil.sh", b"#!/bin/sh\nexit 99\n")?;
    fsx::set_permissions(
        &g,
        &root.join("attachments/evil.sh"),
        std::fs::Permissions::from_mode(0o755),
    )?;
    put(
        &g,
        &root,
        "attachments/launcher.desktop",
        b"[Desktop Entry]\nType=Application\nExec=false\n",
    )?;
    put(
        &g,
        &root,
        ".obsidian/junk.json",
        b"{\"must\":\"stay unchanged\"}\n",
    )?;
    let link = root.join("linked-symlink.md");
    if std::fs::symlink_metadata(&link).is_err() {
        fsx::symlink(&g, Path::new("hub.md"), &link)?;
    }
    println!("{} notes={notes} seed={seed}", root.display());
    Ok(())
}
