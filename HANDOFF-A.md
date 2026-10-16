# Goal A → Goal B

Goal A provides `nib-core`, `nib-vault`, the deterministic fixture, scripts and the
CLI stub. Goal B owns the GTK window and every widget/action. Read
`.planning/SCOPE.md` first: its phase order and discovery decisions still apply.
The branch is `master`; Phases 1–4 have their own completion commits and pushes.

## Starting work

```sh
scripts/check.sh
scripts/dev-run.sh                  # exits 2 until the Phase 5 window exists
scripts/mkfixture.sh --seed 7        # deterministic 3,000-note fixture
NIB_ALLOW_ROOTS= cargo run --release -p nib-vault --example trace -- ~/.cache/nib-fixture
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps -p nib-core -p nib-vault
```

The generated API docs are in `target/doc/nib_core` and `target/doc/nib_vault`.
`check.sh` creates the fixture if absent, runs format/clippy/tests/dependency
boundaries/docs, builds release, runs `nib --version`, and prints the backend
trace. Replace its explicit Phase 5 smoke skip with the window-map check in 5.8.
A warm check remains well below three minutes. There is no check-on-push workflow
(D14). Do not add one as part of Goal B.

Never open, watch or write the live notes vault. Keep `NIB_ALLOW_ROOTS` in every
agent/dev entry point. The reference checkout at `../trek` is read-only.

## Startup and ownership

Use `VaultHandle::open_deferred` for the window startup path. It creates the event
stream and a blocked listing worker, with no root walk, listing or watcher
registration. `read_note` is the synchronous single-note fast path. After
`present()`, an idle callback calls `start_watching()`, `list(root)` and
`start_index()`. Ordinary `open` starts watching/cleanup automatically and remains
useful for tools and tests. `start_watching` is idempotent.

```rust
let (vault, events) = VaultHandle::open_deferred(root, &config, Guard::from_env())?;
let note = vault.read_note(&vault.checked_path(initial_note)?)?;
// Build one buffer from note.text, remember note.fmt and note.read_only; present.
// In the post-present idle callback:
vault.start_watching()?;
vault.list(vault.root());
vault.start_index();
```

Consume `events.recv().await` in a GLib local task. Keep the handle in `App`;
clones are cheap and Send. The last handle cancels services, closes pending
conflict questions and releases watches. Workers never hold widgets. Keep the
`Index` in the app as `Rc<RefCell<Index>>`; take short borrows per event and never
hold one across an await or widget signal.

The config schema follows PROMPT §19: paths are under `config.vault`, including
`new_folder`, `attachments`, `daily_folder`, `daily_format`, `daily_template` and
`templates`. There are no separate `notes`/`daily`/`attachments` config tables.
Config text is preserved by callers; parsing returns a usable config and warnings.
State lives under profile-specific XDG directories, never in the vault. Vault
keys hash canonical native path bytes, including non-UTF-8 directory names.

## Event consumption examples

These are schematic UI dispatch examples; helper names stand for Goal B widgets.
Each `VaultEvent` variant has one dispatch path:

| Event | Example UI handling |
|---|---|
| `Listing { folder, entries }` | `files.set_rows(folder, entries)`; apply `sort::visible` and the current sort to shared cached rows. |
| `Index(event)` | `apply_index_patch(event)` as shown below; enable link features at the first Ready. |
| `NoteChanged { path, text, fmt }` | `tabs.external_change(path, text, fmt)`; reload only an unmodified buffer, otherwise show the conflict notice. |
| `NoteInvalid { path }` | `tabs.external_invalid(path)`; keep modified text intact; an unmodified tab can re-read into the read-only preview. |
| `NoteRemoved { path }` | `tabs.mark_removed(path)`; retain its buffer, and explain that explicit save recreates the file. |
| `NoteRenamed { from, to }` | `tabs.follow_rename(from, to)`; change identity without resetting the buffer or undo history. |
| `Search(event)` | `search.apply_if_current(event)`; discard queued messages from replaced request IDs. |
| `Walk(event)` | `finder.append_if_current(event)`; folders have a trailing slash. |
| `Op(event)` | `operations.apply(event)`; answer Conflict through `vault.answer`, show Progress/Failed, and patch open buffers from Done.summary.edits. |
| `Notice(notice)` | `status.notice(notice.message)`; if `relist_on_focus`, call `reread` on focus-in. |

Apply all index variants:

```rust
match event {
    IndexEvent::Ready(index) => *app.index.borrow_mut() = *index,
    IndexEvent::Upsert(meta) => app.index.borrow_mut().upsert(*meta),
    IndexEvent::Remove(id) => app.index.borrow_mut().remove(&id),
    IndexEvent::Renamed(old, new) => app.index.borrow_mut().rename(&old, new),
    IndexEvent::AttachmentUpsert(path) => app.index.borrow_mut().upsert_attachment(&path),
    IndexEvent::AttachmentRemove(path) => app.index.borrow_mut().remove_attachment(&path),
    IndexEvent::Progress(done, total) => status.indexing(done, total),
}
```

The backend journals changes during the initial parallel build before emitting
Ready, so it cannot replace a newer save with an older parse. A non-UTF-8 file
gets filename-only metadata and remains available in the switcher, with no parsed
links/tags. Attachment paths are available during initial resolution and patchable
thereafter. Ordinary hidden files can be indexed; the four protected directories
and nib temporaries are excluded.

## Notes, saving and operations

Uniform LF/CRLF/CR decode to LF; BOM is separate. Mixed endings stay verbatim in
the buffer. Encode the actual edited text using the remembered `TextFormat`:
`trailing_newline` describes the original; it does not discard a newline the user
explicitly types. No edit plus save is byte-exact. Invalid UTF-8 returns a lossy
**read-only** preview with a notice; `save` also refuses an existing non-UTF-8 file.

Saves resolve symlinks and install a synced temporary file beside the target,
copy permissions, rename and sync the parent. The symlink remains a symlink.
Fresh temporary files are never deleted by cleanup. Stale regular temporary
siblings are recovered on note read; background startup also cleans root temps.

For rename/move:

1. Autosave the relevant buffers before computing `rename_plan`.
2. Ask whether to rewrite (or honor the configured always/never policy).
3. Move with `FileOp::Rename`/`Move`; apply the approved `FileOp::ApplyEdits` plan.
4. On Done, patch open buffers from `summary.edits` inside one user action, with
   highest byte offset first. Convert UTF-8 byte ranges to GTK character offsets;
   do not reload or clear undo. Source IDs are already remapped for folder moves.

`Edit.expected` and `source_hash` reject a stale source snapshot before writing.
Reference-style Markdown destinations are edited once at their definition;
inline destinations preserve angles, labels and fragments. Percent encoding
keeps a bare Markdown destination valid when a filename gains spaces.

Conflicts are once per op: Skip, Replace or Rename (` (1)` filenames). Replace
trashes the old destination first. Trash failure emits Failed and leaves the file
in place; there is no permanent-delete operation. Every mutation routes through
`fsx` and the guard. Protected directories and outside-vault symlink targets are
refused. Operations and edit batches are atomic per file; a later error stops the
remaining work, while completed files remain completed.

`create_note` never overwrites. `ensure_daily` optionally expands a template;
`write_attachment` writes into the configured folder with ` 1`, ` 2`, … suffixes.
`copy_attachment` handles a user-chosen external source through the conflict flow.
It copies bytes/files; GTK texture encoding and image decoding belong to Goal B.

## Finder, search and reading view

`nib-core::fuzzy` constructs immutable title/alias/path, tag, heading and attachment
candidates. Build those when the index/source changes, outside the typing path.
`nib-vault::fuzzy::Matcher::new(candidates)?` starts an actor with nucleo's workers.
`set_query` returns a generation; await `matcher.events()` and discard snapshots
whose `.query` is obsolete. Snapshots carry up to 14 ranked candidates, a total
match count and `.running`. There is no UI tick or polling loop. Keep the matcher
alive while the popover exists; drop cancels it. Even an empty source emits a
settled snapshot.

`search(SearchSpec::parse(query))` and `walk_below(folder)` return handles with IDs.
Keep the current handle alive, and drop/cancel on replacement. Cancellation
synchronizes with sends; already queued old events keep their old IDs.
`SearchHandle::cancel_and_wait` is a blocking CLI/tool helper; the trace uses it to
finish search work before timing rendering. UI query changes use cancel/drop. Search
uses in-process grep, smart case, literal/`r:` regex and path exclusions; it caps
at 2,000 results and emits More. Results contain source lines for display only:
never log those contents. Group by note in the finder.

`render` returns source-mapped `Op` values borrowing unchanged text from the input.
Apply them while that input lives; `Op::into_owned` supports retained/worker results. `Styles::names()` yields semantic tag
names, including headings and indent levels; share a GTK tag table across tabs.
Text link targets are `Arc<LinkTarget>`. Implement both `RenderCtx::resolve` and
`resolve_markdown` using core's corresponding functions, with the current source
ID. Override `resolve_in(source, raw, kind)` with those same resolvers using
its explicit source ID, so links/images inside an embed use the embedded note's
folder. Supply preloaded text through `embed`; embeds are limited to depth one.
`render_section` filters a heading and descendants. Child op line ranges belong
to the embedded note; retain that source ID when wiring child task/link widgets. Anchors cover frontmatter,
images (with width hints), embeds, tasks and rules. Tables are padded monospace
text; footnotes use matching numeric labels at the end. Remote images produce
link text and are never fetched.

Use `checked_path` before image decoding, opening an attachment, or accepting a
user-entered absolute path. For launching, apply `launch::classify` with mode/MIME
facts, and `url_allowed` for external links. This applies to every follow/open
surface, including the reading view. Restore the `NIB_HOST_*` environment for
launched children; `__UNSET=1` means remove that variable in the child.

## Theme and interface refinements

`theme::discover` finds local GTK 3 palettes, bounded local imports, and native GTK
4/dark-variant facts. `NIB_GTK_THEME` overrides the name. After GTK registers its
resources, `VaultHandle::theme_css(name)` recolors its Default base stylesheet and
fixes relative resource URLs. A display-free process has no GTK resource; use
`theme_css_with_base(name, base)` with an extracted base for that case. Follow
Trek's provider priority/Empty-theme handling, prefer a real native dark variant,
and reject a provider on GTK parsing errors. Editor/reading colors still come
from the GtkSourceView scheme, which Goal B supplies.

Contract changes from SCOPE's original guide are now reflected in §5:

- Core uses the pure `nucleo-matcher` crate. Vault uses `nucleo`, whose mandatory
  internal rayon dependency is the sole transitive exception to the dependency ban.
- Stem/alias/tag/unresolved memberships are ordered sets; backlinks expose an
  unordered slice with internal slot bookkeeping. Patches revisit only reverse
  dependencies, with no whole-vault scan or shared-tag vector scan.
- `NoteMeta.text_hash`, `Link.written`, `Edit.expected` and `Edit.source_hash`
  protect rename plans; `link_from` supplies the source missing from the original
  relative-link signature. Case-insensitive extensions, relative wikilinks,
  dotted note stems and shortest attachment basenames have regression tests.
- Ready/Upsert are boxed to keep events small. Index events also include attachment
  upsert/remove; vault events also include NoteInvalid. Search/walk/matcher have IDs.
- `OpSummary` includes exact edits for buffer patching; `open_deferred`,
  `start_watching`, `checked_path`, `ensure_daily` and theme discovery/base APIs are
  additive. Render operations carry compact Styles and shared targets.
- The GLib checksum function is `compute_checksum_for_data` on canonical native
  path bytes. `within_vault` is a pure canonical-input check; actual path resolution
  and confinement remain in vault.

## Verification and remaining ownership

The final measurements and exact check/test counts are in SCOPE §7/§10: 54 tests,
94.52% core line coverage, and a 26.2-second full check including rebuilding.
The deterministic standard fixture is 40.2 MB; every Goal A workstation target is met. The proof
includes a full 3,000-note fixture copy, byte-exact edge saves, the fourteen-link
rename with re-index, all operation guard refusals, real isolated XDG trash,
conflict choices, symlink saves, external watches, cancellation, no pollution,
and 200 SIGKILLs during repeated 1 MB saves. Stale/fresh/symlink temp distinctions
are tested. Doc warnings are errors. Core line coverage exceeds the 90% target;
the installed cargo-llvm-cov and extracted LLVM tool paths are recorded in §10.

The application is deliberately still the Phase 1 CLI stub. No window, source
view, vim wiring, style scheme, icon, clipboard compositor test, GTK provider
validation, UI performance or headless UI check is claimed yet. Those are Goal B
Phases 5–9. Keep all their checklist boxes open until proven. Goal B may add core/
vault APIs as SCOPE §2 allows; log changes and findings in HANDOFF-B.

Owner items remain O-1 (optional repository rename, existing name retained), O-2
(private-repo ARM release runner) and O-3 (Thinkpad measurements). The latter two
belong to Goal C. The Thinkpad budgets, UI budgets and final acceptance checklist
remain unfilled until their owning phases.
