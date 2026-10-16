# Goal B → Goal C

Goal B built the whole GTK application in `crates/nib` (Phases 5–9), the
style schemes, stylesheet, icon and desktop file under `data/`, and the
headless UI checks under `scripts/agent/`. Read `.planning/SCOPE.md` §9 (the
Goal B decision rows) and §10 (Phases 5–9) with this file. Branch `master`;
every phase has its own completion commit and push.

## Running it

```sh
scripts/check.sh                          # fmt, clippy, tests, docs, release, trace, window smoke
scripts/dev-run.sh                        # nib on ~/.cache/nib-fixture, isolated XDG, NIB_TRACE=1
scripts/agent/checks/run-all.sh NAME…     # UI checks; --all runs every one (≈ 9 min)
scripts/agent/latency.sh                  # files column frame timing, 20 000 entries
scripts/agent/smoke.sh                    # window maps; cold and warm start_to_map
```

`scripts/agent/agent-run.sh` is the only way an agent starts nib: headless
`cage`, a private D-Bus session, XDG directories under `~/.cache/nib-agent`,
a fresh rsync copy of the fixture as the vault, `NIB_ALLOW_ROOTS`, and
`NIB_TRACE=1`. Keys are real key events through the Wayland virtual keyboard
(`scripts/agent/vinput`, built from trek's tool); state is `nib --profile
agent --test-dump` (JSON, debug builds only; `q FILTER` in a driver runs jq
over it). A debug-only `win.run "name arg"` D-Bus action runs any registry
entry. `run-all.sh` fails a check on any GTK warning or critical except one
documented GTK 4.22 snapshot warning (SCOPE §10, Phase 7).

## What exists

| Module | Owns |
|---|---|
| `main.rs`, `app.rs` | `GApplication` (`HANDLES_OPEN`, `HANDLES_COMMAND_LINE`, one instance per profile), the `Launcher` (first run, vault choice, forwarded command lines), `App` (one per vault: window, vault handle, index, every component), start-up order, the vault event dispatch, `open_note`/`load_into`/history. |
| `registry.rs` | The one action table: window accelerators, editor chords, files-column sequences, `:` commands, palette rows, `F1`. |
| `commands.rs` | The `:` router (`execute-command`), vim's own commands passed back, tab and quit commands. |
| `editor/` | `Tab` (buffer, view, two-step load, debounces, history, tab button), `vim.rs`, `behaviours.rs` (S4 wiring), `completion.rs`. |
| `tabs.rs`, `session.rs` | Tab bar and stack, 32-tab limit, external changes; session snapshot/save/restore. |
| `files.rs`, `fileops.rs`, `clipboard.rs` | Files column (cursor ≠ selection, find, filter, sort, history, parent column), file operations, trek-compatible clipboard. |
| `finder.rs`, `notes.rs` | Overlay finder and its sources (switcher, palette, `z`, tags, search, templates); daily notes, `:new`, templates. |
| `reading.rs` | Reading view: shared tag table, applier, images, embeds, tasks, modes. |
| `panel.rs`, `status.rs`, `help.rs`, `dialogs.rs` | Right panel, status line, key list, the two dialog kinds. |
| `launch.rs`, `attach.rs`, `config.rs`, `theme.rs`, `debug.rs` | Following links and the D10 guard; attachments; `:config`; theme, fonts and schemes; traces, dump, key injection. |

`crates/nib` is ≈ 10 600 lines of Rust; 24 UI checks, all green at hand-off
(`run-all.sh --all`, ≈ 6 min).

## API changes to Goal A's crates

All additive except one behaviour fix; nothing was removed or renamed.

- `nib_vault::OpSummary.created: Vec<PathBuf>` — where `CopyAttachment` put
  the file (a conflict can rename it). Filled by `ops.rs` for that operation.
- `nib_vault::state::config_text` and `save_config_text` — `:config` reads
  and writes `nib.toml` (outside the vault) through vault's guarded `fsx`.
- `nib_core::render`: a nested list item now starts on its own line (a
  `Newline` before an `Item` that follows text). The `blocks.txt` snapshot
  gains two `NEWLINE`s. Commit `fa3ea14`.

## Goal A findings

1. **Nested list items rendered on their parent's line** (`• first◦ second`).
   Fixed in place as above.
2. **`render_ops_big` timing is noisy**: `check.sh`'s trace printed 14.5 ms
   once (Goal A recorded 7.4 ms; ws target 12 ms). In the app the same
   80 000-op render measured 7–22 ms across runs of one session. Worth a
   look when Goal C re-measures; the reading view no longer depends on it
   for its first paint (SCOPE §9).
3. **`Matcher::new` starts a nucleo pool per matcher** (one thread per core).
   The completion popover makes one per `[[` session and drops it on close;
   the finder one per open. Opening costs 7.8 ms for the note candidates in
   release; typing then costs ≈ 7 µs a key. Fine, but each live matcher is a
   pool of idle threads.
4. **`OpSummary` did not say where a copied attachment landed**: added
   `created` (above).
5. No other contract problems: `open_deferred` → `start_watching`/`list`/
   `start_index` in the post-present idle worked as described; index events
   patch cleanly; own renames emit `NoteRenamed`; own writes never came back
   as `NoteChanged`.

## Toolkit findings (not Goal A; README material for Goal C)

- GtkSourceView 5.20 vim: `:s/a/b/` did not substitute in a headless test,
  and one run logged a mark critical inside it. The insert-mode bar is never
  translated in nib (vim states use `<glib/gi18n.h>`'s `_()`, i.e. the
  application's text domain), so insert detection accepts the English text
  and both translations. Vim keeps one user action open for a whole insert
  session (one undo step per insert), so `end-user-action` only fires on Esc.
- GTK runs a widget's most recently added event controller first; nib's own
  editor controller is re-added after vim's whenever vim attaches.
- GTK 4.22 pops a `GtkPopover` down when the compositor configures it below
  its new minimum size (`gtk_popover_native_layout`): the completion popover
  has a fixed size, and the finder is an overlay, not a popover.
- `set_single_click_activate` also turns on hover selection; finder and panel
  lists use their own click gesture.
- `ListView::scroll_to(FOCUS)` only moves the keyboard when the list already
  has it; `GtkSourceStyle::apply` clears the tag properties a style leaves
  unset; `gdk_pixbuf::Pixbuf` is not `Send` in gtk-rs 0.22; nib forbids
  `unsafe`, which rules out GObject subclasses (their macros expand to
  `unsafe impl`).

## UI checks

`scripts/agent/checks/run-all.sh NAME…` (each starts a fresh session on a fresh
fixture copy; `--all` runs every one):

| Check | Covers |
|---|---|
| `roundtrip` | 5.8: edge files byte-exact after save and type-and-erase; non-UTF-8 read-only, never written. |
| `wq` | 6.1, 6.2: mode text, `:w` claimed before vim's `write`, `:wq`, unknown commands, vim's `:N`. |
| `listcont` | 6.3: list continuation, Tab/Shift+Tab, Ctrl+Enter, auto-pair, headings; English and German UI strings. |
| `follow` | 6.4, D10: executables and `.desktop` refused, nothing started, `file:`/`javascript:` refused, headings, unresolved-create, Ctrl+click. |
| `complete` | 6.5, 6.7: shortest links, aliases, headings, tags, embeds, Esc, Ctrl+Space; "index not ready" with the index held back. |
| `panel` | 6.6: 14 linking notes for hub.md, bold links, Enter opens at the line, outline follows the cursor, F8. |
| `esc` | 6.8, 6.9: Esc in vim leaves the files selection; F1 lists every registry entry and filters. |
| `select`, `column` | 7.2, 7.5: the selection model; every remaining column key, watcher refresh, F5, F7. |
| `rename`, `trash` | 7.4: 14 links rewritten with aliases/headings, open buffer patched, `u` undoes it; XDG trash, linked-attachment notice. |
| `clipboard` | 7.3: yank in nib → paste in the installed trek, and the reverse. |
| `switcher` | 7.6, 7.7: switcher with create row, palette, `z`, `:tag`, vault search with exclusions, open at the line. |
| `reading`, `readtask` | 8.1–8.6: reading view, images, remote image text, embeds, image tab, reading keys, split re-render; task toggle writes `[x]`. |
| `tabs` | 9.1, 9.2: tab bar keys, `:tab`, O and Shift+Enter, an open note switches tabs, untitled `:w`, history with cursor lines. |
| `session` | 9.3: close with Ctrl+Q and reopen restores tabs, cursors, active tab, panels, folder, cursor, sort, reading mode. |
| `autosave` | 9.4: bytes on disk within 1.5 s, tab-switch save; with autosave off `:q` and `:qa` ask. |
| `external` | 9.5: reload in place keeping the line; a modified tab only gets a notice and is not autosaved over; removal; `:w` recreates; rename followed. |
| `daily` | 9.6: Alt+D, `:daily DATE`, `:daily +1`, `:new`, a template filling an empty note. |
| `paste` | 9.7: `wl-copy` PNG pasted with Ctrl+V in insert mode as `Pasted image YYYYMMDDHHMMSS.png`, rendered in reading view; Ctrl+V in normal mode is vim's. |
| `config` | 9.8: `:config`, switching `style_scheme` recolours editor and reading view live, warnings. |
| `cli` | 9.9: a second `nib note.md` opens a tab in the first window, `--new`, `--daily`, folders, another vault refused. |
| `vimoff` | 9.10: vim off — typing, list continuation, Ctrl+Enter, palette for `:`-only commands, go-to-line; every P§17 command registered. |

Not covered headlessly (owner QA): dropping files on the editor (no drag
source in cage), the `:attach` file chooser, the first-run folder chooser,
and the activation token reaching Hyprland.

## Numbers (workstation)

See SCOPE §7. Summary: start-up 163 ms warm (miss, O-4: ≈ 110 ms is GTK's
Vulkan renderer); `big.md` opens in 7–9 ms; files column worst frame 7.9 ms
over 20 000 entries; completion key path 7.3 µs median; switcher visible in
8.4 ms, ranked rows 5.9 ms median; search rows 4 ms; reading view first part
4.4–7.1 ms (whole note ≈ 76 ms later in idle slices); idle RSS 196–215 MB (miss, O-5: ≈ 192 MB before any note; the index and three tabs add ≈ 4 MB; 96 MB with the cairo renderer).

## Open owner items

- **O-4** start-up and **O-5** idle RSS on this workstation (SCOPE §11).
- Drop/`:attach`/first-run chooser/activation token: owner QA (above).
- `de_DE.UTF-8` is not installed here; the non-English list-continuation run
  uses `LANGUAGE=de` on `en_GB.UTF-8`.
- O-1, O-2, O-3 are unchanged (Goal C).
