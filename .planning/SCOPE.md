# nib — SCOPE.md

> Scope and development checklist. **Precedence: SCOPE.md > DISCOVERY.md > PROMPT.md.** PROMPT.md holds the detailed behaviour (section numbers like "P§12" point into it); DISCOVERY.md holds the owner's decisions (D1–D17, gaps G1–G8 = DISCOVERY §2 items 1–8). This file holds the order of work, who does it, and when it's done.
>
> Written 2026-10-16 by Claude (planning), replacing PROMPT §0.1's "the coding agent writes SCOPE first". Agents tick boxes as they go and append to §10 at the end of every phase.

---

## 1. Overview

**nib** is a Markdown notes app for Linux in Rust and GTK 4. It opens the owner's Obsidian vault (`~/Documents/notes`) unchanged and replaces Obsidian and folio as the **daily driver** (D1, D2). It has a Trek-style files column driven by vim motions, a GtkSourceView editor with `VimIMContext`, a reading view built on `GtkTextView`, wikilinks with completion and backlinks, a fuzzy finder, in-process ripgrep-speed search, daily notes, templates and pasted attachments.

**Goals:** Obsidian-compatible on disk (P§3), fast (§7 budgets), minimal (P§2), keyboard-first, safe with notes (DISCOVERY §5).
**Non-goals:** P§2 non-goals, plus permanent delete (D9), trek integration (D16), a public release (D12), and everything in P§27 except the palette (D15).

**Definition of done:** every box in §8 Acceptance is ticked on the acceptance copy, `v0.1.0` is tagged and released, and the owner has carried out DISCOVERY §6 switch-over.

---

## 2. Agents and goals

The work is split **by role, not by vertical slice**. This supersedes D4's slice split and PROMPT §0.2's "each slice ends with a runnable binary". The owner asked on 2026-10-16 for Codex = backend/logic and Claude = frontend/planning/design, grouped so each group runs as one goal.

| Goal | Agent | Phases | Owns | Ends with |
|---|---|---|---|---|
| **A** | **Codex** | 1–4 | `nib-core` (all pure logic), `nib-vault` (all I/O, threads, services), scripts, fixture, a stub `nib` binary | `HANDOFF-A.md`, every Goal A box ticked, `check.sh` green, pushed |
| **B** | **Claude** | 5–9 | `crates/nib` (every widget, action, key and dialog), style schemes, CSS, icon, headless UI checks | `HANDOFF-B.md` including a "Goal A findings" section, `check.sh` green, every UI check green, pushed |
| **C** | **Codex** | 10 | Review of A and B, fixes, man page, `install.sh`, `release.yml`, README, perf table, acceptance, `v0.1.0` | §8 ticked, tag pushed, switch-over handed to the owner |

**Hand-off rules (to keep back-and-forth to two hand-offs):**

1. Goal A builds against the **interface contract in §5**. A deviation is allowed when the contract turns out wrong, but it must be listed in HANDOFF-A §"Contract changes" with the reason.
2. Goal B may make **additive** changes to `nib-core`/`nib-vault`: new functions, new event variants, new fields with defaults. Each one is logged in §10 with "(additive, Goal B)". Changing or removing an existing API goes in HANDOFF-B §"API changes". Goal B never waits on Codex.
3. Goal C fixes what it finds rather than sending it back. Only an owner decision stops the work.
4. There are no review gates inside a goal (D5). Every phase's **Accept** lines plus a green `scripts/check.sh` are the gate.
5. Atomic commits as `codedsleep <purchases91027@pm.me>` on `master`, and **a push at the end of every phase**. The last commit of a phase is `feat(phase-N): …`, which ticks the boxes and appends to §10.
6. Agents never open, read or write `~/Documents/notes` (D11). Every nib run goes through `dev-run.sh`, `agent-run.sh` or `check.sh`.

**Running a goal:** tell the agent *"Run Goal A of `.planning/SCOPE.md` (Phases 1–4). Read SCOPE, DISCOVERY and PROMPT first."* and likewise for B and C.

**Owner pre-flight (before Goal A):** `sudo dnf install gtksourceview5-devel scdoc`, and answer O-1 (repo rename).

---

## 3. Architecture

These are PROMPT §5 and §23 with the clarifications below. The diagram is DISCOVERY §4.

- **`nib-core`** is pure. No `std::fs`, no threads, no `glib`/`gio`/`gtk`. Everything is unit-tested. It holds the domain types, parsing, indexing, resolution, rendering to ops, editor text logic, fuzzy query building, settings and CLI parsing, the palette recolouring, the launch-guard classifier and the clipboard formats.
- **`nib-vault`** owns every byte read or written and every thread. It **may depend on `glib`/`gio`** (for `gio::File::trash`, `glib::compute_checksum_for_str` and XDG dirs) but **never on `gtk4`/`gdk4`/`gdk-pixbuf`**, and never exposes their types. It hands out plain structs and `async-channel` receivers.
- **`nib`** owns widgets, actions, the main context, image decoding (`gdk-pixbuf` on `gio::spawn_blocking`), texture-to-PNG encoding, and anything that needs a display.
- **The index lives on the main thread** as `Rc<RefCell<Index>>`, owned by `nib`. `nib-vault` parses on its threads and sends `NoteMeta` values; `nib` applies them with `Index::upsert/remove/rename`. That means no locks, and each patch is O(size of the note).
- **Fuzzy matching:** query and candidate *building* live in `nib-core::fuzzy`. The nucleo `Matcher` wrapper, which owns nucleo's worker threads, lives in `nib-vault::fuzzy` (the rule is that vault owns every thread).

**Decisions made while scoping (binding):**

| # | Decision | Why |
|---|---|---|
| S1 | `nib-vault` may use `glib`/`gio`, never GTK/GDK. | P§8 already puts `gio::File::trash` in vault. This makes it explicit. |
| S2 | Use **`notify-debouncer-full`** instead of `-mini`. | `-mini` reports no rename pairs, so an external rename (e.g. from trek) of an open note would look like a deletion. `-full` tracks renames by file id. That costs one crate, for correct P§16 behaviour. |
| S3 | **Mixed line endings** are kept verbatim in the buffer (no normalisation), and **non-UTF-8 files** open read-only with a notice and can never be saved. | P§3/§16 promise byte-exact saves, which normalisation can't give for mixed files. A lossy decode would corrupt notes. |
| S4 | Editor text behaviours (list continuation, checkbox, heading cycle, auto-pair, link-at-cursor, completion triggers, word count, outline) are **pure functions in `nib-core::edit`**, wired up by `nib/src/editor/behaviours.rs`. | They become unit-testable, and Codex owns the logic while Claude owns the wiring. |
| S5 | `mkfixture.sh` wraps a Rust example (`cargo run -p nib-vault --example mkfixture -- --seed N DIR`). | Deterministic, fast, and avoids bash generating 3 000 files. |
| S6 | Before Phase 5, `nib` is a **stub binary**: `--version`/`--help` work, and anything else prints `nib: the window arrives in Phase 5` and exits 2. | Lets `check.sh` exercise the binary from Phase 1. |
| S7 | The theme loader keeps trek's dev-run pattern: `NIB_GTK_THEME` override, and `NIB_HOST_*` restores the pre-isolation environment for launched children. | Same reasons as trek (dev-run isolation broke nvim and Brave there). |

---

## 4. Conventions (all agents)

- PROMPT §23 in full: `forbid(unsafe_code)`, clippy pedantic with `-D warnings`, no `unwrap`/`expect` outside tests or proven invariants, doc comments on every `pub` item in core and vault, a `//!` header per module, no comments that restate code.
- Rust 2024 edition, `rust-version = "1.92"`. Toolchain here is Fedora's 1.98.1. GTK `v4_14` and sourceview5 `v5_10` feature floors (CI image: Ubuntu 24.04 = GTK 4.14 / GtkSourceView 5.12).
- **Forbidden dependencies** (enforced by `scripts/forbidden-deps.sh` in `check.sh`): `webkit*`, `libadwaita`, `relm4`, `tokio`, `async-std`, `rayon`, `serde_yaml*`, plus `regex` as a *direct* dependency of `nib-core` (it may come in through `grep-regex` only).
- New runtime crates beyond PROMPT §4, gap G6 and S2 need a row in §9 Decisions with the reason. Dev-only crates are fine with a note in §10.
- `scripts/check.sh` must stay under 3 minutes warm. UI checks run in `check.sh` only as the smoke test; the full UI suite is `scripts/agent/checks/run-all.sh NAME…`. Never run it with no arguments by habit (the owner dislikes 10-minute suites, as with trek).
- **Thinkpad budgets (D6):** on this workstation every metric must come in at **≤ 60 % of its budget**. See §7.

---

## 5. Interface contract (Goal A builds it, Goal B consumes it)

Names are binding; the original guide has been reconciled below with Goal A. Struct fields and complete signatures are documented by `cargo doc`; HANDOFF-A records refinements and consumption examples.

```rust
// ---- nib-core: public values are documented in cargo doc ----------------
NoteId::new(&str) -> Result<NoteId, IdError>; // relative, '/', no Markdown suffix
text::decode(&[u8]) -> Result<(String, TextFormat), TextError>;
text::encode(&str, &TextFormat) -> Vec<u8>;
text::fingerprint(&str) -> u64; // in-process stale-plan detector
parse_note(NoteId, &str, SystemTime, u64) -> NoteMeta;
// NoteMeta has text_hash; Link has raw, written, byte_range and full span.
// Edit { note, byte_range, replacement, expected, source_hash }.
Index::build(Vec<NoteMeta>) -> Index;
Index::build_with_attachments(Vec<NoteMeta>, Vec<String>) -> Index;
Index::{upsert, remove, rename, upsert_attachment, remove_attachment};
Index::backlinks(&NoteId) -> &[(NoteId, u32)]; // display order is unspecified
// by_stem/by_alias/tag/unresolved memberships are BTreeSets; no body cache.
resolve(&Index, &NoteId, &str) -> LinkTarget; // wikilink rules
links::resolve_markdown(&Index, &NoteId, &str) -> LinkTarget;
shortest_link(&Index, &NoteId, LinkFormat) -> String; // root context for relative
links::link_from(&Index, &NoteId, &NoteId, LinkFormat) -> String;
link_text(&str, Option<&str>, Option<&str>, Option<&str>, bool) -> String;
rename_plan(&Index, &NoteId, &NoteId) -> Vec<Edit>; // remaps moved source IDs
render::{render, render_section}; // -> Vec<Op>, source ranges, Styles, Arc targets
render::RenderCtx::{resolve, resolve_markdown, resolve_in, embed};
edit::{continue_list, toggle_task, cycle_heading, auto_pair, link_at,
       completion_trigger, detect_indent, indent_list, word_count, outline};
fuzzy::{Query::parse, switcher_candidates, tag_candidates, completion_candidates};
launch::{classify, url_allowed, within_vault}; // containment needs canonical inputs
clip::{encode_uri_list, decode_uri_list, encode_gnome_copied, decode_gnome_copied};
palette::{parse_gtk3_colors, recolour};
search::SearchSpec::parse(&str) -> SearchSpec;
settings::Config::parse(&str) -> (Config, Vec<Warning>);
settings::DEFAULT_TOML;
cli::parse(impl IntoIterator<Item=String>) -> Result<Invocation, UsageError>;
template::expand; daily::{path, offset}; attach::{pasted_image_name, collision_name};
sort::{Entry, sort_entries, visible, never_shown, age_label};

// ---- nib-vault: all I/O and threads; no GTK/GDK --------------------------
VaultHandle::open(&Path, &Config, Guard) -> Result<(VaultHandle, Receiver<VaultEvent>), VaultError>;
VaultHandle::open_deferred(&Path, &Config, Guard) -> Result<(VaultHandle, Receiver<VaultEvent>), VaultError>;
VaultHandle::start_watching(&self) -> Result<(), VaultError>; // idempotent
VaultHandle::{root, checked_path, watcher_ready, needs_relist, listing_reads};
VaultHandle::read_note(&self, &Path) -> Result<NoteText, VaultError>; // sync fast path
VaultHandle::save(&self, &Path, &str, &TextFormat) -> Result<Generation, VaultError>;
VaultHandle::{list, reread, start_index}; // background event results
VaultHandle::search(&self, SearchSpec) -> SearchHandle; // keep until cancelled
VaultHandle::walk_below(&self, &Path) -> WalkHandle;
SearchHandle::{id, cancel, cancel_and_wait}; // only the last blocks; tools only
VaultHandle::op(&self, FileOp) -> OpId;
VaultHandle::answer(&self, OpId, ConflictAnswer);
VaultHandle::create_note(&self, &NoteId, &str) -> Result<PathBuf, VaultError>;
VaultHandle::ensure_daily(&self, NaiveDate) -> Result<PathBuf, VaultError>;
VaultHandle::write_attachment(&self, &str, &[u8]) -> Result<PathBuf, VaultError>;
VaultHandle::copy_attachment(&self, &Path) -> OpId;
VaultHandle::theme_css(&str) -> Option<String>; // after GTK resource registration
// FileOp: Copy, Move, Trash, Rename, CreateNote, CreateFolder, ApplyEdits, CopyAttachment.
// OpSummary { files, links, edits }; no permanent-delete operation.
// NoteText { text, fmt, read_only, notice }.
VaultEvent::{Listing, Index, NoteChanged, NoteInvalid, NoteRemoved, NoteRenamed,
             Search, Walk, Op, Notice};
IndexEvent::{Progress, Ready(Box<Index>), Upsert(Box<NoteMeta>), Remove, Renamed,
             AttachmentUpsert, AttachmentRemove};
// Search/Walk events carry request IDs; discard queued obsolete events in the UI.
Guard::{from_env, confined, unconfined, check};
state::{paths, vault_key, find_vault_root, load_config, Session, TabState,
        append_crash, cleanup_stale_tmp};
log::init(); trace_span!("name");
fuzzy::Matcher::new(impl IntoIterator<Item=Candidate>) -> Result<Matcher, VaultError>;
fuzzy::Matcher::set_query(&self, &str) -> u64;
fuzzy::Matcher::events(&self) -> Receiver<Snapshot>; // query ID, page, counts, running
// Matcher owns its actor/pool; no polling or UI tick needed.
theme::{discover, roots, theme_css, theme_css_with_base};
```

---

## 6. Development phases and task checklist

Legend: **Deps** = tasks that must be done first (inside a goal, phase order implies the previous phase). **Accept** = how the task is proven. `ws` = this workstation.

---

### GOAL A — Codex (backend, logic) · Phases 1–4

#### Phase 1 — Workspace, text I/O, safety rails

- [x] **1.1 Trek reference.** `gh repo clone codedsleep/trek ~/code/trek` (read-only; never commit there). Read its README, `crates/trek-fs` (folder listing, watcher, finder, job guard) and `scripts/agent/` (check pattern). **Accept:** §10 lists the shapes borrowed and the ones rejected.
- [x] **1.2 Workspace.** Root `Cargo.toml` with members `crates/{nib-core,nib-vault,nib}`, shared `[workspace.dependencies]` pinned for every P§4 crate plus `gdk-pixbuf`, `log`, `notify-debouncer-full`, dev `tempfile`. `[workspace.lints]` with clippy pedantic plus the P§23 allows. Release profile per P§4. `#![forbid(unsafe_code)]` in all three crates. `.gitignore` (`target/`; `.planning/` stays tracked, D12). MIT `LICENSE`. **Accept:** `cargo build --workspace` succeeds; `cargo tree -p nib-core` shows no glib/gio/gtk.
- [x] **1.3 Core types** (`nib-core::model`). `NoteId` (separator normalising, `.md` stripping, `Arc<str>`), `Tag` (lowercase, nested by `/`), `Heading {level, text, slug, line}`, `Link`, `LinkTarget`, `NoteMeta`, error enums (`thiserror`). **Accept:** unit tests for id normalisation, slug rules (P§12) and tag folding.
- [x] **1.4 Text format** (`nib-core::text`, S3). Covers BOM, LF/CRLF/CR, trailing newline, mixed (verbatim) and non-UTF-8 (`Err`). **Accept:** `encode(decode(b)) == b` for every combination, plus mixed and empty files; non-UTF-8 returns `TextError::NotUtf8`.
- [x] **1.5 Settings** (`nib-core::settings`). `Config` per P§19, `Config::parse → (Config, Vec<Warning>)`, unknown keys warn, bad values fall back and warn, `DEFAULT_TOML` with comments (and gap G8's path). **Accept:** tests for unknown keys, wrong types and partial files; `parse(DEFAULT_TOML)` gives `Config::default()` with zero warnings.
- [x] **1.6 CLI** (`nib-core::cli`). Every P§18 form, `--profile`, `--vault`, `--new`, `--daily`, `--version`, `--help`, and the debug-only `--test-keys`/`--test-dump` (parsed always; gated in `nib`). **Accept:** table-driven tests, including bad combinations producing usage errors.
- [x] **1.7 State paths** (`nib-vault::state`). XDG config/state dirs per profile, `vault_key` (SHA-256 of the canonical root, first 16 hex chars, via `glib::compute_checksum_for_str`), `find_vault_root` (P§3: argument → nearest ancestor with `.obsidian/` or `.nib/` → the file's folder). **Accept:** tempdir tests for each root rule and a pinned vault key.
- [x] **1.8 Guard** (D11). `NIB_ALLOW_ROOTS` (colon-separated, `~` expanded). Set-but-empty confines writes to the fixture and the temp dir; unset means unconfined. `Guard::check` before every write (enforced by routing all writes through one `fsx` module). **Accept:** tests for allowed, denied, `..` escapes and symlinked escapes; `grep -rn 'fs::write\|File::create\|rename(' crates/nib-vault/src` hits only `fsx`.
- [x] **1.9 Atomic save** (`nib-vault::fsx::save`, P§16, G5). Resolves symlinks, writes the temp file `.{stem}.nib-tmp` beside the **target**, then `fsync`, copies permissions, `rename`s and `fsync`s the directory. Returns a `Generation`. `cleanup_stale_tmp` removes `.*.nib-tmp` older than 1 minute without following symlinks. **Accept:** tests show the symlink stays a symlink, permissions are kept, a read-only directory gives an error with the original untouched, and stale/fresh temp files are handled.
- [x] **1.10 Config, session, crash log, logging.** `load_config` creates `nib.toml` from `DEFAULT_TOML` on first run (atomic). `Session` (P§15 fields) loads and saves atomically. `append_crash(report)`. `log::init` is a ≈40-line `log::Log` sink (`NIB_LOG`), and `trace_span!` prints `name=… ms` when `NIB_TRACE=1`. **Accept:** round-trip tests; a corrupt `session.toml` gives defaults plus a warning, never an error.
- [x] **1.11 Stub `nib` binary** (S6). `main` → `cli::parse`; `--version` prints `nib 0.1.0-dev`, `--help` prints usage; the panic hook writes the crash log and then chains to the default hook. **Accept:** `nib --version` exits 0; a forced test panic appends to the crash log in a temp `XDG_STATE_HOME`.
- [x] **1.12 Scripts.** `scripts/check.sh` runs fmt check, clippy (all targets, `-D warnings`), `NIB_ALLOW_ROOTS= cargo test --workspace`, `forbidden-deps.sh`, a release build and `nib --version`, plus a smoke-test step that prints `skipped until Phase 5`. `scripts/mkfixture.sh` (S5) builds `~/.cache/nib-fixture`: everything in P§24 plus the edge files `crlf.md`, `bom.md`, `mixed-eol.md`, `no-trailing-newline.md`, `latin1.md` (non-UTF-8), `linked-symlink.md` → a real note, `attachments/evil.sh` (exec bit), `attachments/launcher.desktop`, and one note linked from exactly 14 others (`hub.md`, for §8). `scripts/dev-run.sh` gets XDG isolation under `~/.cache/nib-dev`, `NIB_TRACE=1`, `NIB_ALLOW_ROOTS`, `NIB_HOST_*` export (S7) and `NIB_GTK_THEME` from the desktop's `gtk-theme`. **Accept:** `check.sh` green; two `mkfixture.sh --seed 7` runs produce an identical `find … -type f -exec sha256sum` listing.

#### Phase 2 — Markdown, links and every pure algorithm

- [x] **2.1 Frontmatter** (`nib-core::frontmatter`, P§6). A leading `---` block; `title`, `aliases`, `tags` as a scalar, flow list or block list; returns the span; anything else is ignored. **Accept:** tests pin the exact subset, including CRLF, BOM-prefixed, unterminated and empty blocks.
- [x] **2.2 `parse_note`** (P§6, §12). One pulldown-cmark pass with the P§4 options. Extracts headings with slugs; links (wikilink, embed, relative Markdown link with `<…>` form, external, attachment-by-extension) with line, column and byte range; tags from body text (hand scanner, skipping code spans and fences, no regex) plus frontmatter; `^block` ids. **Accept:** fixture-fragment tests; `parse_note` on a 5 KB note p50 ≤ 60 µs on ws (timing test, `#[ignore]` by default, run by the trace example in 4.3).
- [x] **2.3 `Index`** (P§6, §12). `build`, `upsert`, `remove` and `rename`, each O(size of the note) or better. Maintains `by_stem`, `by_alias`, `backlinks`, `tags`, `unresolved`. When a new note's stem or alias matches unresolved links, those links resolve (looked up through `unresolved`, with no full scan). **Accept:** tests that upsert-then-remove leaves the index equal to its pre-state, and that creating `Foo.md` resolves existing `[[Foo]]` links.
- [x] **2.4 `resolve`** (P§12 rules 1–4, heading by slug, block by id, a missing heading still resolves the note). **Accept:** tests on the fixture's ambiguous stems (same folder, shortest path, alphabetical), aliases, case and `folder/Note` forms; every case in P§3's table.
- [x] **2.5 `shortest_link` and link writing.** Covers the `shortest`/`relative`/`absolute` formats and a `link_text(target, heading, block, alias, embed)` builder. **Accept:** tests with unique and duplicated stems.
- [x] **2.6 `rename_plan`** (P§6, §8). Handles a note or a whole folder. Preserves `|alias`, `#heading`, `^block`, the embed `!`, and the Markdown-link form (re-relativised from each source). Edits are sorted by descending byte offset per file. **Accept:** the `hub.md` case gives 14 edits across 14 notes; every link variant has a test; applying the plan and re-parsing resolves everything to the new id.
- [x] **2.7 `render`** (P§11). `Op::{Text{str, tags}, Anchor{Image|Embed|Task|Rule|Frontmatter}, Newline}`, where each block op carries its source line range (for the task toggle and section re-render). Covers headings, emphasis, strong, strike, inline code, code blocks with a language label, quotes, callouts (title + type label), lists with indent levels and `• ◦ ▪` glyphs, ordered numbers, tasks, links (resolved / unresolved / external), rules, tables (monospace column-padded), footnotes (superscripts + definitions at the end), and the frontmatter summary (`— N properties —`). `RenderCtx` resolves links and supplies embed text (depth 1). `render_section` handles `![[Note#Heading]]`. **Accept:** snapshot tests per element in `crates/nib-core/tests/snapshots/` (plain text, `UPDATE_SNAPSHOTS=1`); building ops for `big.md` takes ≤ 12 ms on ws.
- [x] **2.8 Editor text logic** (`nib-core::edit`, S4). `continue_list` (`-`, `*`, `1.`, `- [ ]`, numbers increment, an empty marker removes itself, indentation kept); `toggle_task` (flip, or add `[ ]` on a plain list line); `cycle_heading` (P§9 rules); `auto_pair` (configured pairs); `link_at(text, offset)` (wikilink / Markdown link / bare URL span); `completion_trigger(line, offset)` (`[[`, `#` at a word boundary, `![[`, partial link) with no allocation on the miss path; `detect_indent`; `word_count` (Unicode words outside fences); `outline`. **Accept:** table-driven tests; `completion_trigger` ≤ 30 µs on a 2 000-character line (timing test).
- [x] **2.9 Template, daily, attachment names.** `template::expand` (`{{date}}`, `{{date:FMT}}`, `{{time}}`, `{{title}}`; unknown left as is; nothing executes); `daily::path` and `offset`; `attach::pasted_image_name(now)` gives `Pasted image YYYYMMDDHHMMSS.png`, with collisions getting ` 1`, ` 2`, …. **Accept:** tests, including shell-looking template input left verbatim.
- [x] **2.10 Sorting and visibility** (`nib-core::sort`). Natural order with folders first, modified time newest first, reverse, the hidden-file toggle, the never-shown set (`.obsidian`, `.nib`, `.git`, `.trash`), and `age_label` (`3h`, `2d`). **Accept:** tests (`note2` before `note10`; never-shown even with hidden files on).
- [x] **2.11 Fuzzy building blocks** (`nib-core::fuzzy`). `Query::parse` (fzf syntax → nucleo pattern), `switcher_candidates(&Index, recent)` (title, `alias → title`, path), `tag_candidates` (with counts), `completion_candidates(kind, &Index, target)` (notes + aliases; headings after `#`; tags; attachments first for `![[`). **Accept:** tests on candidate content and order.
- [x] **2.12 Launch guard and containment** (D10). `launch::classify(name, mode, content_type) → Launch::{Open, Refuse(reason)}`, `url_allowed` (only `http`, `https`, `mailto`), `within_vault(root, path)` (canonical prefix check). **Accept:** tests for exec bit, `.desktop`, `.AppImage`, `.run`, `.sh`, the four MIME types, `file:`, `javascript:`, and `../` escapes.
- [x] **2.13 Clipboard formats** (`nib-core::clip`). `text/uri-list` and `x-special/gnome-copied-files` (`copy`/`cut` + URIs), with percent-encoding matching trek byte for byte (read trek's code). **Accept:** round-trip tests, plus a test on a string captured from trek's output (stored in fixtures).
- [x] **2.14 Palette port** (D15). `palette::parse_gtk3_colors` and `recolour`, ported from `trek-core::palette` along with its tests. **Accept:** trek's test cases pass, and NordArc's `@define-color` set parses.
- [x] **2.15 Search query** (`nib-core::search`). `SearchSpec::parse` handles smart-case literal, `r:` regex and `-word` path exclusions. **Accept:** tests.
- [x] **2.16 Coverage.** `cargo llvm-cov -p nib-core` reported in §10, target ≥ 90 % lines. If `cargo-llvm-cov` isn't installed, install it with `cargo install` and note that. **Accept:** number recorded.

#### Phase 3 — Vault services (threads, watching, search, ops)

- [x] **3.1 `VaultHandle` skeleton** (§5). Worker threads, one `async-channel` per handle for events, a generation registry for self-writes, `VaultError` (`thiserror`). **Accept:** `open` on the fixture returns in ≤ 5 ms (no walking on open).
- [x] **3.2 Listing service** (P§8). A dedicated listing thread does `read_dir` + metadata, sorted by `nib-core::sort`. LRU of the 12 most recent folders, each watched; `list` serves from the LRU without touching disk, `reread` forces a read. **Accept:** tests show `list` twice reads once; a 20 000-entry tempdir folder lists in ≤ 60 ms on ws.
- [x] **3.3 Watcher** (P§16, S2). `notify` recursive on the root via `notify-debouncer-full` at 150 ms, coalesced per path, mapped to listing-cache updates, index reparse, and `NoteChanged`/`NoteRemoved`/`NoteRenamed`. nib's own writes are dropped by generation, and `.*.nib-tmp` by name. `ENOSPC` raises one `Notice` carrying the `sysctl fs.inotify.max_user_watches=…` line and switches to the re-list fallback flag. **Accept:** tempdir tests for an external edit (one `NoteChanged`), an own save (no event), an external rename (`NoteRenamed`), and a burst of 50 writes (≤ 2 events per path).
- [x] **3.4 Index builder** (P§12). `ignore` walk (skips `.git`, `.obsidian`, `.nib`, `.trash`, honours `.gitignore`/`.ignore`), a chunked work queue over `available_parallelism()` std threads, `Progress` events at most every 50 ms, and `Ready(Index)`. Incremental reparse on watcher events and saves sends `Upsert`/`Remove`/`Renamed`. **Accept:** fixture index ready in ≤ 240 ms on ws (release); 30 000 notes (`mkfixture --notes 30000`) ≤ 2.4 s.
- [x] **3.5 Search service** (P§13). `grep-searcher` + `grep-regex` with the `ignore` parallel walk, results streamed in batches grouped by note, capped at 2 000 then a `More` event, cancelled on `cancel()`/drop (checked between files). **Accept:** the first batch for a common word arrives in ≤ 60 ms on ws; a cancelled search sends nothing after `cancel` returns.
- [x] **3.6 Finder walk and matcher.** `walk_below` streams vault-relative paths in batches. `nib-vault::fuzzy::Matcher` wraps nucleo, injects candidates, and streams ranked snapshots over the channel. **Accept:** the first snapshot for a 3 000-item list arrives in ≤ 6 ms after a query change on ws.
- [x] **3.7 File ops** (P§8, D9). Copy, move, trash (`gio::File::trash`; a failure means refuse, **never** fall back to delete), rename, create note or folder (never overwrite), and `ApplyEdits` (rename-plan edits written atomically, reporting notes and links rewritten). Conflicts are asked once per op (skip / replace / rename ` (1)`) through `answer`. `Progress` events (`copying 3/12`). Every write goes through the Guard. No delete operation exists. **Accept:** tempdir tests for each op, each conflict answer, a guard refusal, and a trash failure leaving the file in place.
- [x] **3.8 Notes I/O.** `read_note` (sync; decode with S3), `create_note` (from optional template text, never overwrites), the daily ensure helper (create if missing from `daily.template`), `write_attachment` (atomic, into `attachments.folder`, collision suffix), `copy_attachment` (an op with a conflict ask). **Accept:** tests, including non-UTF-8 → `NoteText { read_only: true }`.
- [x] **3.9 Theme discovery** (D15). Finds a GTK 3 theme by name in `~/.themes`, `~/.local/share/themes` and the XDG data dirs, reads `gtk-3.0/gtk.css` and its `@import`s, and returns `palette::recolour` of GTK 4's base CSS (taken from trek's approach). `NIB_GTK_THEME` overrides the name. **Accept:** a test with a fake theme dir; `NordArc-Theme` on this machine produces CSS.
- [x] **3.10 External-change support.** `NoteChanged` carries the decoded text and format. A content hash suppresses events where the bytes didn't change. **Accept:** `touch` alone sends nothing; an edit sends one event with the new text.

#### Phase 4 — Integration proof, performance numbers, hand-off

- [x] **4.1 Integration tests** (`crates/nib-vault/tests/`, tempdir copies of the fixture). Byte round trip for every edge file; `hub.md` rename plan applied (14 notes rewritten, re-index resolves); trash; guard refusals for every op; symlink save; conflict answers; a **no-pollution test** that diffs a full file listing before and after a scripted session of ops (only the expected paths change, nothing under dot-folders). **Accept:** green under `check.sh`.
- [x] **4.2 Crash-safety test** (§8 kill -9 item). A child process saves a 1 MB note in a loop and the parent `SIGKILL`s it at random points 200 times. Each time the note is exactly the old or the new bytes, and the next `open` removes any stale temp file older than the threshold (the test fakes the mtime). **Accept:** green, ≤ 20 s.
- [x] **4.3 Trace example.** `cargo run --release -p nib-vault --example trace -- DIR` prints `index_ready`, `parse_note_p50/p99`, `search_first_batch`, `render_ops_big`, `list_20k` and `matcher_first`. **Accept:** numbers recorded in §7 (ws column), all ≤ 60 % of budget.
- [x] **4.4 Docs.** Doc comments on every `pub` item, a `//!` per module, `cargo doc --no-deps -p nib-core -p nib-vault` with no warnings, and §5 updated in place to match what was built. **Accept:** the doc build is clean.
- [x] **4.5 `HANDOFF-A.md`.** What exists, how Goal B uses each service (with one short example per `VaultEvent`), contract changes, measured numbers, known gaps, and anything Goal B must decide. **Accept:** file committed; every Goal A box ticked; §10 entries for Phases 1–4; pushed.

---

### GOAL B — Claude (frontend, design, planning) · Phases 5–9

Goal B starts by reading HANDOFF-A and skimming Goal A's code. Anything wrong goes into HANDOFF-B §"Goal A findings" and is fixed in place if it blocks.

#### Phase 5 — App shell, design system, first editable note

- [x] **5.1 Design assets.** `data/style-schemes/nib-nord.xml` (default; Nord palette, and must style `def:heading`, `def:emphasis`, `def:string`, `def:comment`, `text` for the reading view) and `nib-light.xml`; `crates/nib/src/nib.css` (status line, tab bar, files-column rows and dim label, finder sizing, selection and cursor rows from `@theme_selected_bg_color`, P§20 only); `data/io.github.codedsleep.Nib.svg` (a simple nib glyph, legible at 16 px). **Accept:** a screenshot of the scheme on a fixture note with the NordArc palette loaded, attached to §10.
- [x] **5.2 Application and window** (P§7, §23). `gtk::Application` with `HANDLES_OPEN | HANDLES_COMMAND_LINE`; `Rc<App>` with a `RefCell` per concern and `Weak` captures; layout `Box(v)` > `Paned` { files | `Paned` { centre | right } } + status line, proportions 1 : 4 : 1.5; no header bar; title `note — vault — nib`. Theme: `VaultHandle::theme_css` loaded as the theme provider, with the dark variant preferred when the theme has one (trek's behaviour). Scheme search path appended from the binary's data dir. **Accept:** the window maps on the fixture; F8/F9 toggle panes.
- [x] **5.3 Action registry** (P§13 palette, §17). One table of `(name, label, accel, scope, handler)` drives the window `ShortcutController` (bubble phase), the `:` command router, the palette and F1. Every P§17 entry is registered as the feature lands. **Accept:** a unit test that every registry entry has a label and no two entries share an accel within a scope.
- [x] **5.4 Status line** (P§7). Labels for mode/command bar, pending keys, path + `●`, word count, `line:col`, index status or a notice (4 s, also logged). **Accept:** shown in `--test-dump`.
- [x] **5.5 Files column, browsing** (P§8). `ListView` over a `gio::ListStore` of `BoxedAnyObject`; the factory binds two label texts only; breadcrumb; `j k gg G Ctrl+U/D/B/F h l Enter o` (`O` arrives in Phase 9 with tabs); pending-sequence display; fed by `VaultHandle::list`. Never-shown entries come from core. **Accept:** a 20 000-entry folder scrolls with no frame over 16 ms (WAYLAND_DEBUG frame timing, trek's `latency.sh` method).
- [x] **5.6 Editor, single tab** (P§9). `sourceview5::View` in a `ScrolledWindow`; every P§9 view setting from config (font from GSettings `monospace-font-name`, `max_width` centring box); open via `read_note` with S3 (read-only banner for non-UTF-8); `Ctrl+S` → `save` with the remembered `TextFormat`; `line:col`; `●`. **Accept:** opening `big.md` reaches first paint in ≤ 10 ms on ws.
- [x] **5.7 Start-up path** (P§21). *(Built as specified; the ws target is missed — 163 ms warm, cause in §10 Phase 5 and O-4.)* Only config, session and the one note are read before `present()`; after `present`, an `idle_add_local_once` starts listing, index and watcher. `NIB_TRACE` prints `start_to_map`. A dev-only stall probe warns on main-thread work over 4 ms. **Accept:** `start_to_map` ≤ 90 ms on ws on the fixture.
- [x] **5.8 Headless check harness** (P§24, trek pattern). Debug-only `--test-keys` (an injecting action, compiled out of release) and `--test-dump` (JSON: tabs, cursor, files-column cursor and selection, status text, focus, panel state). `scripts/agent/agent-run.sh` (headless `cage`, fresh fixture copy, `NIB_ALLOW_ROOTS`), `scripts/agent/checks/lib.sh`, and `run-all.sh NAME…`. The `check.sh` smoke step goes live: the window maps and `start_to_map` is printed. **Accept:** `roundtrip.sh` (open and save each edge file byte-exact) green; `check.sh` green.

#### Phase 6 — Vim, editor behaviours, links UI

- [x] **6.1 VimIMContext** (P§10). Capture-phase key controller per view, with `set_client_widget`. `command-bar-text` goes to slot 1 (`NOR` when empty) and `command-text` to slot 2. `editor.vim = false` gives a plain view. A spike first confirms that `execute-command` can claim `:w` before the `write` signal, and the result is recorded in §10. **Accept:** `--test-dump` shows `-- INSERT --` after `i`.
- [x] **6.2 Command router** (P§10, §17). `execute-command` → registry: `w w! wq x q q! qa e {note} e! {N}` now, the rest as their features land. Unknown commands show `not an editor command: …` and return `false`. **Accept:** `wq.sh` check (edit, `:wq`, bytes on disk, tab closed).
- [x] **6.3 Behaviour wiring** (S4, G3). `nib/src/editor/behaviours.rs` calls `nib-core::edit` for list continuation (insert detection by `dgettext("gtksourceview-5", "-- INSERT --")`), auto-pair, `Ctrl+Enter` checkbox, `Ctrl+Shift+Up/Down` headings, and `Tab`/`Shift+Tab` on list lines. Word count and outline are computed on a 300 ms idle debounce. **Accept:** `listcont.sh` passes under `LANG=en_GB.UTF-8` **and** `LANG=de_DE.UTF-8`.
- [x] **6.4 Follow link** (P§9, D10). `Alt+Enter`, `:follow` and `Ctrl+click`. A wikilink opens the note, or creates an unresolved one in `notes.new_folder` with a notice. `#heading` and `^block` scroll to the target (with a notice if it's missing). External links go through `url_allowed`, then launch with the `NIB_HOST_*` environment restored. Attachments go through `classify`, so images open and executables are refused with a notice. **Accept:** `follow.sh`: a link to `attachments/evil.sh` shows the refusal notice and spawns nothing (process count unchanged).
- [x] **6.5 Completion popover** (P§9, §12). nib's own `Popover` + `ListView`, opened by `completion_trigger` or `Ctrl+Space`, ranked by `vault::fuzzy::Matcher`. `Tab`/`Enter` accept, `Esc` dismisses, `Up`/`Down`/`Ctrl+N`/`Ctrl+P` move. Accepting writes `shortest_link` and puts the cursor after `]]`. **Accept:** `complete.sh` (typing `[[amb` and accepting inserts the shortest unique link); the key path adds ≤ 30 µs (trace).
- [x] **6.6 Right panel** (P§12). Backlinks grouped by source, with the line text and the link text bolded, plus an "unresolved in this note" section. Outline is live from the buffer and follows the cursor's section. `Tab` switches tabs, `Alt+B` focuses backlinks, `Enter` or click opens at the line. Updates on index patches and tab switch. **Accept:** the panel for `hub.md` lists 14 sources.
- [x] **6.7 Index wiring.** Holds `Rc<RefCell<Index>>`, applies `IndexEvent`s, shows `indexing n/total`, and turns link features on at `Ready`. **Accept:** while indexing, the editor is usable and completion says `index not ready` instead of being empty.
- [x] **6.8 Key list.** `F1` (and `~` in the files column) lists the registry with a filter entry. **Accept:** the list count equals the registry count.
- [x] **6.9 Esc routing** (G4). **Accept:** `esc.sh`: with a files-column selection, focusing the editor, `i`, `Esc` leaves the selection intact and the mode at `NOR`.

#### Phase 7 — File-manager parity and the Finder

- [ ] **7.1 Dialogs** (P§7). A yes/no question and a one-line entry, as transient `gtk::Window`s; `y`/`Enter` and `n`/`Esc`. **Accept:** used by 7.4; covered by its checks.
- [ ] **7.2 Selection model** (P§8). The cursor is not the selection. `Space`, `v` (visual), `Ctrl+A`, `Ctrl+R`, `Esc`. Commands act on the selection, else on the row under the cursor. **Accept:** `select.sh`.
- [ ] **7.3 Clipboard** (P§8). `y x p P Y`, using a `gdk::FileList` content provider plus `x-special/gnome-copied-files` via `nib-core::clip`; `cl` yanks a wikilink as text; `cp cn cr` copy paths and names. **Accept:** `clipboard.sh` yanks in nib and pastes in the installed trek (and the reverse) inside one cage session, then checks the files.
- [ ] **7.4 Ops UI** (P§8, D9). `d` trashes, with a notice if the attachment is still linked (giving the count). `a` creates (`name/` for a folder; `name` becomes `name.md` and opens). `r` renames with `rename_plan` → `Update N links in M notes? [y/n]` (respecting `links.rewrite_on_rename`) → `ApplyEdits`; open buffers are **patched in place** (undo survives). Conflict prompts; progress in the status line. **Accept:** `rename.sh` (rename `hub.md`: 14 rewritten, aliases and headings kept, an open linker buffer patched, then `u` undoes the patch); `trash.sh`.
- [ ] **7.5 Column completeness.** `/ ? n N` find, `f` filter, `,n ,m ,r` sort, `.` hidden files, `,m` linemode, `gh gd ga gt gr`, `H L` folder history, `F5`, and the `F7` parent column (1 : 3 width). Watcher events refresh the column. **Accept:** `column.sh` covers each key once.
- [ ] **7.6 Finder widget** (P§13). A top-centre `Popover` at 60 % width, an entry, a `ListView` of at most 14 rows, and a `FinderSource` trait. Keys: `Up`/`Down`/`Ctrl+N`/`Ctrl+P`/`Tab`/`Shift+Tab`, `Enter`, `Shift+Enter` (new tab, from Phase 9), `Esc`. **Accept:** the popover is visible ≤ 18 ms after `Alt+O` on ws.
- [ ] **7.7 Finder sources.** Quick switcher (`Alt+O`, `:o`): candidates built once from the index and kept, recent-first order, and a `Create "query"` row. Command palette (`Alt+P`): fuzzy over the registry with accel hints. `z` finder (`walk_below`). `:tag` (tags with counts, then that tag's notes). Vault search (`Alt+F`, `:find`): autosave first, streamed and grouped results, cancel on query change, a `more…` row, open at the line. **Accept:** `switcher.sh` (create from the switcher); first ranked results ≤ 6 ms after a keystroke; search first page ≤ 60 ms (end to end, trace).

#### Phase 8 — Reading view

- [ ] **8.1 Tag table** (P§11). Built once and shared across tabs; colours from the active scheme's styles; reading font from GSettings plus 1 pt (`editor.reading_font`). **Accept:** switching scheme via config recolours both views.
- [ ] **8.2 Op applier.** One `begin_user_action`/`end_user_action` pass. Anchors: a `Separator` for rules, picture placeholders, clickable task glyphs, embed regions, and a frontmatter expander. Links are clickable; unresolved links are dim with a dashed underline (click creates); external and attachment clicks go through D10. **Accept:** `big.md` renders in ≤ 30 ms on ws, including insertion.
- [ ] **8.3 Images.** `gdk_pixbuf::Pixbuf::from_file_at_scale` on `gio::spawn_blocking` to the view's width or the `|N` hint; placeholder until loaded; vault-local only (remote images show their link text); image files open in a reading tab from the files column. **Accept:** the fixture's 20 images render; no network calls.
- [ ] **8.4 Embeds.** `![[Note]]` and `![[Note#Heading]]` render at depth 1 in a quote-styled region with a title line; a nested embed shows as a link. **Accept:** fixture embed notes render as described.
- [ ] **8.5 Task toggle.** Clicking a glyph applies `toggle_task` to the source line in the buffer, autosaves and re-renders only that block's ops. **Accept:** `readtask.sh` writes `[x]` to disk.
- [ ] **8.6 Modes.** `Alt+E` toggles source/reading in the same slot (scroll mapped by line ratio). `Alt+Shift+E` splits side by side with a `Paned`, re-rendering on a 400 ms idle, with ratio scroll-sync. Reading keys `j k gg G Ctrl+D Ctrl+U` are nib's own handler, `Enter` follows the focused link, and `i` returns to source. **Accept:** typing in split view adds no measurable key-path cost (trace).

#### Phase 9 — Tabs, session, daily notes / templates / attachments, CLI

- [ ] **9.1 Tab bar** (P§15). `ToggleButton`s in a `Box`, ellipsised titles, `●`, middle-click closes. `Alt+T`, `Alt+W`, `Alt+[`, `Alt+]`, `:tab N|new|close|next|prev`, `O` and `Shift+Enter` open in a new tab. Opening a note that's already open switches to its tab. At 32 tabs, the oldest unmodified tab closes. Untitled tabs prompt for a name on `:w`. **Accept:** `tabs.sh`.
- [ ] **9.2 History** per tab: `Alt+Left`/`Alt+Right` with the cursor remembered. **Accept:** covered by `tabs.sh`.
- [ ] **9.3 Session** (P§15). Saved on close and every 30 s while dirty: tabs, cursors, active tab, pane sizes, panel visibility, column folder and cursor, sort. Restored on open. **Accept:** `session.sh` (close and reopen restore exactly).
- [ ] **9.4 Autosave** (P§16). 1 s after the last edit, on tab switch, window focus-out, `Alt+E`, before a rename plan and before a search. With autosave off, `:q` asks and `:qa` asks once, listing the notes. **Accept:** `autosave.sh` (bytes on disk within 1.5 s).
- [ ] **9.5 External changes** (P§16). An unmodified buffer reloads in place with the cursor kept by line, plus a notice. A modified buffer gets a `changed on disk — :e! to reload, :w to overwrite` notice. Removed → notice, and `:w` recreates the file. Renamed (S2) → the tab follows. ENOSPC fallback: re-list on focus-in. **Accept:** `external.sh`.
- [ ] **9.6 Daily notes and templates** (P§14). `Alt+D`, `:daily [±N|date]`, `Alt+N`/`:new`, `Alt+Shift+T`/`:template` (finder over the templates folder; inserts at the cursor or fills an empty buffer). **Accept:** `daily.sh`.
- [ ] **9.7 Attachments** (P§14, G2). `Ctrl+V` with a texture on the clipboard, **in insert mode or with vim off**, saves `Pasted image …png` via `write_attachment` and inserts `![[…]]`; otherwise the key goes to vim. `:paste-image` and a palette action work in any mode. Dropping files copies them in and inserts a link (a vault `.md` gets a link without copying). `:attach` opens a `gtk::FileDialog`. **Accept:** `paste.sh` (`wl-copy < png` inside cage); the name matches Obsidian's format.
- [ ] **9.8 `:config`** opens `nib.toml` in a tab. Saving it reloads the config and applies fonts, scheme, wrap, autosave and margins live; warnings appear in the status line. **Accept:** changing `style_scheme` recolours without a restart.
- [ ] **9.9 CLI and single instance** (P§18). Every invocation form; `--profile` gives app id `io.github.codedsleep.Nib.p_NAME` and separate state; `nib foo.md` reaches the running instance with the activation token; first run shows a folder chooser; `data/io.github.codedsleep.Nib.desktop` with `MimeType=text/markdown;`. **Accept:** `cli.sh` (a second `nib note.md` opens a tab in the first window).
- [ ] **9.10 Remaining commands and the vim-off audit.** Every P§17 `:` command exists (`backlinks outline files read edit split rename trash reindex help version paste-image`). With `editor.vim = false`, every feature is reachable through chords or the palette. **Accept:** `vimoff.sh`; the registry lists every P§17 row.
- [ ] **9.11 Measure and hand off.** Idle RSS with the fixture open and 3 tabs (target ≤ 90 MB; record the real number either way); every Goal B budget in §7; `HANDOFF-B.md` (what exists, API changes, Goal A findings, UI checks list, open owner items O-*). **Accept:** every Goal B box ticked, §10 entries for Phases 5–9, pushed.

---

### GOAL C — Codex (review, ship) · Phase 10

#### Phase 10 — Review, harden, ship

- [ ] **10.1 Review.** Read SCOPE, both hand-offs and the full diff since the initial commit. Check every task's Accept line against the code, act on HANDOFF-B's Goal A findings, and work through DISCOVERY §5 (`grep` for `remove_file`/`remove_dir_all` on vault paths; `strace -f -e trace=network` on a fixture run shows no network sockets; launch guard; guard coverage). Fix in place, with findings and fixes in §10. **Accept:** §10 review log; `check.sh` and every UI check green.
- [ ] **10.2 Man page and desktop data.** `data/nib.1.scd` built with `scdoc`, listing every invocation form, the key tables from the registry (generated, not hand-written), files and environment variables (`NIB_LOG`, `NIB_TRACE`, `NIB_ALLOW_ROOTS`, `NIB_GTK_THEME`). **Accept:** `man ./nib.1` renders.
- [ ] **10.3 `install.sh`** (D13). Uses `gh release download`, verifies `SHA256SUMS`, and installs bin, desktop file, icon, man page and style schemes into `~/.local` (or `--prefix`); supports `--version vX`, `--uninstall` and `-y`; checks for the GtkSourceView 5 runtime and prints the `dnf` line. The `scripts/install.sh` local build does the same from a checkout. **Accept:** a dry run into a temp prefix on ws; uninstall leaves nothing behind.
- [ ] **10.4 `release.yml`** (D14, O-2). `v*` tags build x86_64 on `ubuntu-24.04`, plus aarch64 if the arm runner is free for private repos (otherwise drop it and note that in the README), and upload a release with `SHA256SUMS`. **Accept:** the first tag run is green.
- [ ] **10.5 README and CHANGELOG.** Pitch, features, install (gh), the P§17 key tables, "Coming from Obsidian", "Vim" limits (from §10 notes), and the performance table. `CHANGELOG.md` 0.1.0 in Keep-a-Changelog form. **Accept:** committed.
- [ ] **10.6 Thinkpad numbers** (D6, O-3). Run `NIB_TRACE=1` on the fixture on the thinkpad (ssh + headless `cage` if that works, else ask the owner to paste a `dev-run.sh` run). Fill §7's thinkpad column and the README table. **Accept:** every budget met, or the miss is recorded with its cause.
- [ ] **10.7 Acceptance.** `cp -a ~/Documents/notes ~/.cache/nib-acceptance && git -C … init && commit` (the only time an agent touches the vault, D11), then run §8 there. **Accept:** §8 ticked or each failure explained.
- [ ] **10.8 Release.** Bump to `0.1.0`, commit `Release 0.1.0`, annotated tag `v0.1.0`, push, watch the release run, and run `install.sh -y --version v0.1.0` on ws. Hand the switch-over (DISCOVERY §6) to the owner. **Accept:** release published; the installed `nib --version` reports `0.1.0`.

---

## 7. Performance budgets

Each row lists the thinkpad budget (P§21, D6), the workstation target (≤ 60 %), the phase that proves it, and the measured numbers.

| Metric | Thinkpad budget | ws target | Proven in | ws measured | thinkpad measured |
|---|---|---|---|---|---|
| `parse_note`, 5 KB note | ≤ 100 µs | ≤ 60 µs | 2.2 / 4.3 | 13.582 µs p50 / 19.246 µs p99 | |
| `completion_trigger` key path | ≤ 50 µs | ≤ 30 µs | 2.8 / 6.5 | 0.615 µs (core); in the app, line read + trigger + matcher query per keystroke: 7.3 µs median, 17.6 µs p95, 40 µs worst of 107 (release) | |
| Render ops build, `big.md` | — | ≤ 12 ms | 2.7 / 4.3 | 7.367 ms | |
| Index ready, 3 000 notes | ≤ 400 ms | ≤ 240 ms | 3.4 / 4.3 | 100.947 ms (3,058 notes; 40.2 MB fixture) | |
| Index ready, 30 000 notes | ≤ 4 s | ≤ 2.4 s | 3.4 | 417.603 ms (30,058 notes) | |
| Search, first page | ≤ 100 ms | ≤ 60 ms | 3.5 / 7.7 | 0.714 ms (first streamed batch) | |
| Matcher first snapshot | ≤ 10 ms | ≤ 6 ms | 3.6 / 7.7 | 0.196 ms | |
| Start → window mapped, note editable | ≤ 150 ms | ≤ 90 ms | 5.7 | **163 ms warm (miss)**; nib's own work before `present()` ≈ 10 ms, the rest is exec ≈ 20, GTK init ≈ 30, Vulkan renderer init ≈ 110 (RTX 4090 + llvmpipe ICDs). Cold first start in a fresh headless bus 420–440 ms (portal activation). | |
| Open a 10 000-line note, first paint | ≤ 16 ms | ≤ 10 ms | 5.6 | 7.2–9.0 ms typical, outliers to 15 ms (two-step insert; whole `set_text` was 14–16 ms) | |
| Switcher popover visible | ≤ 30 ms | ≤ 18 ms | 7.6 | | |
| Reading render, `big.md`, with insertion | ≤ 50 ms | ≤ 30 ms | 8.2 | | |
| Files column, 20 000 entries | 60 fps | no frame > 16 ms | 5.5 | worst frame 7.9 ms, median 1.0–1.5 ms over j, held j, Ctrl+F, Ctrl+D, G, gg | |
| Idle RSS, fixture, 3 tabs | ≤ 90 MB | ≤ 90 MB | 9.11 / 10.6 | | |

---

## 8. Acceptance checklist (Phase 10.7, on the acceptance copy)

From P§26, adjusted by discovery:

- [ ] After a scripted session of edits, `git status` in the acceptance vault shows only the notes and attachments touched, and nothing under `.obsidian/` (the owner repeats this after a real week, D2).
- [ ] Every §7 budget met on the thinkpad and printed by `NIB_TRACE=1`; the numbers are in the README.
- [ ] Every P§17 key works as the table says (minus `D`, D9); `F1` lists them from the registry.
- [ ] Every P§17 `:` command exists, plus `:paste-image`.
- [ ] With `editor.vim = false`, every feature stays reachable.
- [ ] Renaming a note that 14 others link to prompts, rewrites all 14 with aliases and headings kept, and patches open buffers with undo intact.
- [ ] `kill -9` during a save leaves the note as the old or the new bytes; a stray `.nib-tmp` is cleaned on the next open.
- [ ] A screenshot pasted from `grim`/`wl-copy` is saved as an attachment, `![[…]]` is inserted, and it renders in reading view.
- [ ] A file yanked in trek pastes with `p` in nib's column, and the reverse.
- [ ] `scripts/check.sh` green; every UI check green; `cargo clippy` clean with pedantic.
- [ ] Idle RSS with the fixture open is under 90 MB (or the measured number and its cause are recorded).
- [ ] DISCOVERY §5 security checklist fully ticked.
- [ ] Following a link to an executable attachment is refused; `file:`/`javascript:` links don't launch.
- [ ] A non-UTF-8 note opens read-only and can't be saved; CRLF, BOM and mixed-ending notes round-trip byte-exact.

---

## 9. Decisions log

The decisions so far are D1–D17 (DISCOVERY §1), G1–G8 (DISCOVERY §2) and S1–S7 (§3 above). New decisions are added here with a date, the agent, and the reason.

| Date | By | Decision | Reason |
|---|---|---|---|
| 2026-10-16 | owner via Claude | Split by role: Codex = core + vault + ship/review, Claude = GTK app + design. Replaces D4's slice split and P§0.2's vertical slices. | Owner request: minimal back-and-forth, agents grouped by role. |

| 2026-10-16 | Claude | UI checks press real keys through the Wayland virtual keyboard (`scripts/agent/vinput`, from trek) inside headless cage; `--test-keys` (debug only) forwards its words to that tool. | GTK 4 has no public way to synthesise key events into a widget or `VimIMContext`; the virtual keyboard exercises the same path as a person's keys, including capture-phase controllers. |
| 2026-10-16 | Claude | A note longer than 128 KB that opens at its top inserts its first ≈ 64 KB before the first frame and the rest in the next idle; every reader of the buffer text (`Tab::text`) inserts the remainder first. | One `set_text` of `big.md` (650 KB) costs ≈ 10 ms in GtkTextBuffer alone, so the 10 ms first-paint target could not be met with P§21's "set_text once". |
| 2026-10-16 | Claude | The Markdown language is set on the first tab in the post-present idle (later tabs get it at creation). | `LanguageManager::language` parses every language file's header (≈ 35 ms). The DISCOVERY §9 risk mitigation names this. The scheme stays before `present()` (≈ 0 ms) so colours are right in the first frame. |
| 2026-10-16 | Claude | `editor.max_width` widens the view's side margins instead of putting the view in a width-requested box. | A `TextView` inside a box inside a `ScrolledWindow` is no longer the scrollable child and renders every line. |

| 2026-10-16 | Claude | nib's editor key controller runs window chords (`Alt+B`, `F8`, `Ctrl+S`, …) as well as editor ones, and is re-added after vim's controller whenever vim attaches. | GTK runs a widget's most recently added controller first; in normal mode vim consumed `Alt+B` as `b` and `Alt+Enter` as `Enter` before the window's bubble-phase shortcuts saw them. |
| 2026-10-16 | Claude | Vim insert mode is detected by the bar text matching `-- INSERT --`, `dgettext("gtksourceview-5", …)` or `dgettext(None, …)` (G3 refined). | GtkSourceView 5.20's vim states use `<glib/gi18n.h>`'s `_()`, which translates in the application's text domain, so the bar stays English in nib under every locale while the library-domain lookup is translated. Matching only the translation would have broken list continuation for non-English users. |
| 2026-10-16 | Claude | Completion is re-evaluated on the buffer's `changed` signal, not `end-user-action`. | VimIMContext keeps one user action open for a whole insert session (one undo step), so `end-user-action` only fires on `Esc`. |
| 2026-10-16 | Claude | `:` commands nib owns run in a high-priority idle after vim's `execute-command` returns. | `:wq` closes the view vim is still dispatching for; closing inside the signal produced a `gtk_text_buffer_delete_mark` critical. |

| 2026-10-16 | Codex | Core depends on `nucleo-matcher`, vault on `nucleo`; the dependency check permits rayon only transitively through nucleo. | nucleo 0.5 unconditionally depends on rayon; the specified matcher cannot satisfy a blanket transitive ban. Core uses only pure pattern machinery and never starts matcher threads. |

---

## 10. Implementation log

Each phase appends an entry: date, agent, commits, what was done, numbers measured, deviations, and anything the next goal must know.

### Phase 1 — 2026-10-16, Codex

Commits: `f34881b` (planning), `1194088` (workspace/core), `b2197dd` (guard/state), `99190d9` (fixture/scripts), plus the phase completion commit.

- Borrowed Trek's three-crate split, read-once folder cache/event model, weak lifetime/cancellation pattern, GNOME/URI clipboard wire format, guard policy and headless-check conventions. Rejected remote/GVFS, icons/type sniffing during listing, desktop services, permanent delete, and per-widget ownership of I/O.
- `scripts/check.sh` green: format, pedantic clippy, 9 tests, dependency check, release build and `nib --version`. Core dependency tree contains no GLib/GIO/GTK or thread pool. Debug forced-panic subprocess appended a crash report under temporary XDG state.
- Two `mkfixture.sh --seed 7` runs: 3,082 identical file hashes (including the symlink target); fixture contains 3,000 generated notes plus edge/daily/template files. Existing fixture contents are replaced deterministically; use a fresh directory when changing note count.
- Atomic save, stale cleanup, permissions, read-only directory, symlink escapes and session recovery proven in tempdirs. `fs::rename` appears only in `fsx` in production source.
- Contract refinement: uniform endings decode to LF; mixed endings stay verbatim. `trailing_newline` is descriptive and does not erase an explicit newline edit. GLib's actual checksum API is `compute_checksum_for_string`.
- Local cargo-fmt resolves a broken stale executable in `~/.cargo/bin`; `check.sh` selects the installed `/usr/bin/rustfmt` unless RUSTFMT is set. No host tool files changed.


### Phase 2 — 2026-10-16, Codex

Commits: `081ab1a` (pure algorithms and snapshots), `af88ef7` (fixture backlink correction), plus the phase completion commit.

- Implemented frontmatter, single-pass extraction, reverse-dependency index patches, Obsidian resolution, source-aware link writing, note/folder rename plans, reading operations, editor behaviors, candidate construction, calendar/templates, sorting, search specs, launch policy, clipboard formats and palette transfer. Rename tests cover fourteen linkers and Markdown destinations with spaces (percent encoded).
- `scripts/check.sh` green: 28 core tests plus 5 vault tests; two explicit release probes are ignored in the ordinary suite. Core coverage **94.61% lines** with cargo-llvm-cov 0.9.1 (installed for this task). Matching Fedora LLVM 22.1.8 tools were downloaded/extracted under `/tmp/nib-llvm-tools`, without system installation.
- Release probes: 5 KB parse p50 **8.389 µs**; completion on 2,000 chars p50 **0.623 µs**; 10,000-line render ops **11.218 ms**. The initial render missed 12 ms; compact `Styles`, static tag names, shared link targets, capacity reservation and source-line locality brought it under budget. Full fixture measurements follow in Phase 4.
- Contract refinements: `render::Op` carries `Styles` (iterate `names()`) and `Arc<LinkTarget>` to avoid per-run style strings/large enum copies. `Index.attachments` supplies embed candidates. `link_from` supplies the source omitted by the root-relative `shortest_link` guide signature. `within_vault` is a pure check on canonical inputs; vault performs canonicalization. Template/daily invalid formats return unchanged placeholders/errors instead of panicking.
- Palette behavior was independently implemented against Trek's reference cases, keeping nib's MIT code independent of Trek's GPL implementation. Clipboard fixture pins the reference encoder's exact newline/percent-encoding representation; compositor interop remains Goal B's UI check.

### Phase 3 — 2026-10-16, Codex

Commits: `59ef76f` (index/render performance), `a4c8aa2` (services), `74ce0f6` (service tests/trace), plus the phase completion commit.

- Implemented cheap cloneable handles with final-drop shutdown, a dedicated 12-folder LRU listing worker, recursive paired-rename watcher, self-write generations/content hashes, parallel ignore-aware parsing, journaled patches during the initial build, cancellable parallel grep and finder walks, an event-driven nucleo actor, guarded ops/conflict handling, read-only non-UTF-8 previews, daily/attachment helpers and bounded theme imports.
- `scripts/check.sh` green: 40 normal tests (plus the isolated trash subprocess invoked by its parent). Watch tests prove own-save suppression, touch suppression, an external edit, paired rename, removal and 50-write coalescing; an injected watch-limit error gives exactly one actionable fallback notice. Listing twice shares the same Arc and reads once.
- Trace: open **0.093 ms**, index 3,057 notes **45.559 ms**, index 30,057 notes **325.348 ms**, first search **0.720 ms**, listing 20,000 entries **24.783 ms**, matcher **0.252 ms**, rendering 10,000 lines **10.186 ms**. The original 30k run took 13.4 s due to per-note sorting of common tag tables; the bulk-build path sorts them once.
- Real NordArc palette + the installed GTK base resource produced **157,007 bytes of CSS**. The resource was extracted with the system `gresource` tool for this display-free proof; the app calls `theme_css` after GTK registers its resources. `theme_css_with_base` supports display-free callers/tests.
- GIO refuses trash on this system's `/tmp` tmpfs. Success/replace tests therefore use a temporary directory on the checkout filesystem and an isolated child-process XDG data home. Refusals leave the file intact; no permanent-delete fallback exists.
- Contract refinements: boxed Ready/Upsert event payloads, a `NoteInvalid` event for external encoding changes, request IDs on streamed search/walk and matcher snapshots, `checked_path` for frontend image/launch containment, an explicit `ensure_daily`, and `OpSummary.edits` for buffer patching. Listings include ordinary hidden entries; core/UI filters preferences without a re-read. `cancel()` prevents future sends; the UI discards already queued events by request ID.

### Phase 4 — 2026-10-16, Codex

Commits: `f3ef5e5` (rewrite preconditions/index memberships), `0186beb` (borrowed rendering), `328f364` (external edits/startup), `c636280` (state), `2e3b2e1` (integration/crash proofs), `e90306a` and `b8092cc` (file-path/read refusals), `49bc6ed` (40 MB fixture), `91b8ecd` (embed source context), `39ebdba` (isolated trace), plus the phase completion commit.

- **All Goal A boxes complete.** Final `scripts/check.sh` green: **54 tests** (32 core, 22 vault), plus the parent-invoked isolated trash child and 200 crash children. The two timing probes and two child entry points are deliberately ignored by the generic test runner; parents invoke the children, and the trace exercises timing. Format, pedantic clippy, dependency boundaries, warning-free docs, release build and version check pass. Final full check **26.171 s**, including a rebuild (under the 3-minute warm limit).
- Full-fixture integration copies the generated vault into a tempdir, round-trips CRLF/BOM/mixed/no-final-newline/symlink files byte-exact, refuses non-UTF-8 saves, moves `hub.md`, applies fourteen edits across fourteen notes and re-indexes. A full before/after manifest (bytes, types, symlink targets, modes) contains exactly the expected changes, with nothing under protected folders and no temp pollution. Every mutation variant has a guard-refusal proof. Folder rename follows open paths and remaps edits even without a watcher.
- Crash proof: a child repeatedly saves a 1 MB note, is SIGKILLed at 200 deterministic varied offsets, and every resulting note is exactly old or new. An explicit run took **2.526 s** and recovered **153** stale temps; all runs stayed below 20 s. Stale cleanup works on background startup and for a nested note's temporary sibling, never following temp symlinks. Read-only directories, directories passed as notes and unreadable pre-save sources refuse without changing existing bytes.
- Integration hardening: source fingerprints plus expected destination bytes refuse stale rename plans; reference definitions are rewritten once; dotted/case/relative note names and shortest attachment basenames resolve; attachment events update completions. Shared tag/alias/unresolved memberships use ordered sets, and backlink slots use swap removal, avoiding degree-sized scans on a note patch. Incremental tables are compared against fresh rebuilds through 80 updates.
- Watcher proofs now include another editor's atomic replace and reverting to older nib-written bytes. Self-write generations are consumed, rather than suppressing a later external revert. Own moves update paths/index immediately; external rename targets pass containment before reads. Added `open_deferred`/`start_watching` for Phase 5's post-present startup order, and journaled attachment updates alongside note updates.
- Renderer operations borrow unchanged input runs; `into_owned` supports worker/retained results. The context receives an embedded note's source ID for its relative links/images. Snapshot tests cover borrowed/owned equivalence, all requested elements, matching numeric footnotes, depth-one embeds and independent parent/child resolution. Mixed CR/LF mapping uses the permitted `memchr` crate.
- The original generated corpus measured only 31 MB; enlarged it to **40,205,169 bytes**. Two seed-7 runs produced identical **3,082** file hashes (including the symlink target). The enlarged 30k fixture is on `/tmp` tmpfs; the standard 3k fixture is under the home Btrfs cache. Both are warm-cache workstation measurements. The trace now waits for cancelled search workers before timing the independent renderer; it does not charge leftover search work to rendering.
- Final 40 MB trace: open **0.075 ms**; 3,058-note index **100.947 ms**; 30,058-note index **417.603 ms**; parse p50/p99 **13.582/19.246 µs**; completion **0.615 µs**; search first batch **0.714 ms**; render **7.367 ms**; 20k listing **15.599 ms**; matcher **0.196 ms**. Every Goal A workstation target is met. Numbers remain printed by `check.sh`, without failing on scheduling noise (D7). GUI-only and Thinkpad rows remain for their owning phases.
- Core line coverage **94.52%**, measured with cargo-llvm-cov 0.9.1 and extracted Fedora LLVM 22.1.8 under `/tmp/nib-llvm-tools`. `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps -p nib-core -p nib-vault` is clean. The actual Trek clipboard encoder was run from a temporary dependent crate, and its captured output matches the committed URI fixture byte-for-byte; the Trek checkout remains unchanged.
- §5 now reflects the actual public API. `HANDOFF-A.md` explains startup, every VaultEvent, index patches, cancellation, source formats, buffer edits, source-aware reading operations, containment/theme resources, refinements and Goal B/C ownership. The binary remains the required CLI stub and the GUI smoke remains explicitly skipped until Phase 5. No real-vault or repository-name changes were made.

### Phase 5 — 2026-10-16, Claude

Commits: `3bda05e` (schemes, stylesheet, icon), `f811421` (window, start-up, files column, editor tab), `5fb9ab3` (agent harness, roundtrip, latency, smoke), plus the phase completion commit.

- **App shell.** `crates/nib`: `app.rs` (Launcher per process, `App` per vault, event dispatch), `theme.rs`, `registry.rs`, `status.rs`, `files.rs`, `editor.rs`, `tabs.rs`, `help.rs`, `debug.rs`. One `Rc<App>` built with `Rc::new_cyclic`; every signal handler holds a `Weak<App>`. The vault opens with `open_deferred`; the post-present idle runs `start_watching`, the first listing, `start_index`, then the Markdown language and the `vault.last` write (a line edit that keeps the rest of `nib.toml` byte for byte).
- **Design (5.1).** `data/style-schemes/nib-nord.xml` and `nib-light.xml` validate against GtkSourceView's `styles.rng`; `data/io.github.codedsleep.Nib.svg` checked at 16 px. Screenshot with the NordArc palette loaded: `.planning/shots/phase5-nib-nord-nordarc.png` (headings frost, emphasis purple, code green, list markers yellow, links blue; the window chrome is NordArc's recoloured GTK stylesheet).
- **Registry (5.3).** One static table of `(name, label, keys, scope, commands, handler)`. Window entries become `ShortcutController` shortcuts in the bubble phase; files entries are vim-notation sequences matched by the column's capture-phase key controller (pending keys shown in slot 2). Unit tests: every entry labelled, no key twice in a scope, no command twice, no files sequence a prefix of another. The `F1`/`~` key list (6.8) came forward as a transient window, because it is the registry's reader.
- **Files column (5.5).** `ListView` over a `gio::ListStore` of `BoxedAnyObject(Row)`; the factory binds the name and (with `,m`, Phase 7) the age. Rows are swapped in one `splice`. `scroll_to(FOCUS)` only moves the keyboard when the list already has it, so `Alt+1` grabs first. `scripts/agent/latency.sh` (trek's WAYLAND_DEBUG method plus a GTK frame-clock probe, `NIB_FRAMES=1`) on a 20 000-entry folder: worst frame 7.9 ms; key-to-commit medians 2–15 ms include waiting for cage's 60 Hz frame callback.
- **Editor (5.6).** Font from GSettings `monospace-font-name` (schema looked up first; `gio::Settings::new` aborts on a missing schema), reading font from `font-name` + 1 pt, both through one CSS provider. Non-UTF-8 notes show a banner and refuse saves. Opening `big.md` from the files column: 7.2–9.0 ms to first paint after the two-step insert (§9); replacing a 650 KB buffer with a small note still costs ≈ 11 ms of GtkTextBuffer deletion (not a budget row; noted for later).
- **Start-up (5.7): missed on this workstation.** Warm `start_to_map` 163 ms (median of three, `scripts/agent/smoke.sh`). Breakdown from trace points (since removed): exec to `main` 15–35 ms, GTK init and app registration 20–40 ms, theme 7 ms, window build 3 ms, `present()` to map 110 ms (Vulkan renderer: GTK picks the RTX 4090, the loader also initialises llvmpipe), map to first paint 15 ms. `GSK_RENDERER=ngl` gives ≈ 175 ms and `cairo` ≈ 105 ms; nib does not switch renderers (DISCOVERY §9). The first start in a fresh private bus is 420–440 ms because it activates the settings portal; the owner's session has it running. Thinkpad (Intel, Mesa) is the judge (D6); Goal C measures it. Owner item O-4.
- **Harness (5.8).** `scripts/agent/agent-run.sh` (headless cage, private bus, `~/.cache/nib-agent` XDG dirs, fresh rsync copy of the fixture, `NIB_ALLOW_ROOTS`, `NIB_TRACE=1`), `checks/lib.sh`, `checks/run-all.sh NAME…` (refuses to run everything without `--all`; any GTK warning or critical fails a check), `checks/roundtrip.sh` (green: CRLF, BOM, mixed endings, no final newline and the symlinked note byte-identical after save and after type-and-erase; `latin1.md` read-only, save refused, bytes untouched; no temp files). `--test-dump` is a forwarded command line printing JSON through `ApplicationCommandLine::print_literal`. A debug-only `win.run "name arg"` action runs any registry entry over D-Bus. `check.sh` runs `scripts/agent/smoke.sh` in place of the skip line: warm 4-check run 45 s.
- The crash hook called `sourceview5::major_version()`, which asserts an initialised GTK main thread, so a panic before GTK started aborted without writing the log. It now checks `gtk::is_initialized_main_thread()` first; `NIB_TEST_PANIC=1 nib --version` appends to the crash log again.

### Phase 6 — 2026-10-16, Claude

Commits: `2c69132` (vim, router, behaviours, completion, links, panel, selection, dialogs), `03a4799` (checks), plus the phase completion commit.

- **Spike (6.1).** Read GtkSourceView 5.20's `gtksourcevimimcontext.c`: `execute-command` is a RUN_LAST signal with a `true_handled` accumulator whose class handler emits `write`/`edit`, so a handler returning true claims `:w` before `write`. Confirmed live: `wq.sh` connects a debug-only logger to `write` and asserts it never fires while `:w` saves. GtkSourceView's own commands are `colorscheme delete j join nohl redo set sort u undo y yank`, line numbers, ranges, `s…`, `/` and `?`; nib passes those back (`commands::vim_builtin`) and says `not an editor command: …` for anything else. `sourceview5::init()` is now called at start-up.
- **Vim limits found** (for the README, Goal C): `:s/a/b/` did not substitute in a headless test and once logged a mark critical inside GtkSourceView; the insert bar is untranslated (above).
- **Router (6.2).** `w w! write wq x q quit q! qa qa! qall e edit e! follow check backlinks outline files help version` exist now. `:w NAME` names an untitled tab; plain `:w` on one asks for a name (entry dialog). Closing the last tab leaves an empty one. `:q` with autosave on saves first; with autosave off it asks; `:qa` asks once listing the notes.
- **Behaviours (6.3).** `editor/behaviours.rs` passes the current line (Tab/Shift+Tab: the whole text, for the note's indent unit) to `nib-core::edit` and applies the edit as one user action, keeping the cursor on its text. Auto-pair steps over a closer that is already next. `listcont.sh` green in English and with `LANGUAGE=de` (de_DE is not installed; `en_GB.UTF-8` is).
- **Follow (6.4).** `launch.rs`: wikilinks through `resolve`, Markdown links through `resolve_markdown`, bare URLs; `#heading`/`^block` scroll via the index (a missing one opens the note with a notice); unresolved links create the note in `vault.new_folder` (`""`, `"current"`, or a folder); attachments go through `checked_path`, then `launch::classify` with the mode bits and GIO's sniffed content type; web and mail links only. Children get the `NIB_HOST_*` environment (S7). Before the index is ready, wikilinks say so instead of creating notes. `follow.sh`: `evil.sh` and `launcher.desktop` refused, no child process, `file:`/`javascript:` refused, heading, unresolved-create and Ctrl+click all green.
- **Completion (6.5).** One popover per window, parented to the current view, fixed 420 × 308 px: GTK 4.22 pops a popover down when the compositor configures it smaller than its new minimum (`gtk_popover_native_layout`), which happened when rows arrived after an empty reopen. Candidates come from `nib-core::fuzzy::completion_candidates`, a `nib-vault::fuzzy::Matcher` per completion session (opening one costs 7.8 ms once per `[[`, release), obsolete snapshots dropped by query id. Accepting writes `link_from`/`shortest_link`, `link|alias` for alias rows, heading text, tag, or the attachment's basename when unique, and steps past `]]`. A lone `#` (a heading being typed) does not open tag completion; Ctrl+Space does. Key path: §7.
- **Panel (6.6).** Backlinks sorted by source then line, the linking lines read on `gio::spawn_blocking` through `VaultHandle::read_note` and the link spans (from the source's `NoteMeta`) bolded in Pango markup; refreshes are queued once per idle on index events, note opens and tab switches, and skipped when nothing changed. Outline rebuilt on the 300 ms idle from the buffer; the current section is the outline's selection. `panel.sh`: hub.md shows `(14 linking notes)` and fourteen sources.
- **Index wiring (6.7).** `complete.sh` second session (`NIB_TEST_INDEX_DELAY_MS=20000`, debug only): the popover says `index not ready` and typing goes on.
- **Keys (6.8)** `F1`/`~` count equals the registry (`esc.sh`). **Esc (6.9)** `esc.sh`: with two rows selected, focusing the editor, `i`, `Esc` keeps both and returns to `NOR`.
- Pulled forward from Phase 7: the two dialog kinds (7.1) and the selection model (7.2: Space, v, Ctrl+A, Ctrl+R, Esc). Rows remember the label they are bound to so a mark repaints one row; nib forbids `unsafe`, and the GObject subclass macros expand to `unsafe impl`.

---

## 11. Open questions

- **O-1** Rename the repo to `codedsleep/nib` and the folder to `~/code/nib`? This is the owner's call; best done before Goal A starts.
- **O-2** aarch64 release on a private repo: Goal C decides (10.4).
- **O-3** Thinkpad trace over ssh + headless `cage`: Goal C tries it (10.6).
- **O-4** (Goal B, 2026-10-16) The workstation misses the 90 ms start target: 163 ms warm, of which ≈ 110 ms is GTK's Vulkan renderer initialising on the RTX 4090 (plus the llvmpipe ICD). nib's own pre-`present()` work is ≈ 10 ms. Accept the workstation number if the thinkpad meets ≤ 150 ms, or decide on a renderer policy (nib does not set `GSK_RENDERER` by itself).
