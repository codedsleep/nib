# nib — PROMPT.md

> Working name: **nib** (binary `nib`, app id `io.github.codedsleep.Nib`). Rename freely; the spec uses it throughout.

## 0. Read me first (instructions for the coding agent)

1. Before writing any code, write `.planning/SCOPE.md`: a one-page restatement of this spec's goals, non-goals, the slice order in §25 and the acceptance checklist in §26. Keep it current as decisions are made; it is the file later review passes read.
2. Build in the vertical slices of §25, in order. Each slice ends with a runnable binary and a green `scripts/check.sh`. Do not start a slice before the previous one's "done when" holds.
3. `codedsleep/trek` (https://github.com/codedsleep/trek) is the reference for the file-manager half: its keys, its "a folder is read once, listed on its own thread, watched for changes" model, its clipboard interop, its three-crate split and its headless key-driven checks. Read its README before slice 1 and its `crates/trek-fs` before slice 4. Borrow the shapes; do not vendor its code wholesale — nib is a smaller program.
4. "Minimal and fast" is the acceptance test for every decision. When two designs are both correct, choose the one with fewer widgets, fewer crates, fewer threads and fewer bytes on the heap. Every feature in this spec exists because it is on the path from "open Obsidian" to "never open Obsidian again"; add none that is not.
5. Measure, do not guess: §21 has numbers. `NIB_TRACE=1` must print the timings that prove them.

## 1. Elevator pitch

A Markdown notes app for Linux, in Rust and GTK 4, that opens an Obsidian vault unchanged and replaces Obsidian for daily writing: a Trek-style file column on the left driven with vim motions, a GtkSourceView editor with real vim emulation in the middle, a reading view instead of a web renderer, wikilinks with completion and backlinks, a fuzzy quick switcher, ripgrep-speed vault search, daily notes, templates and pasted attachments — and nothing else. It starts in the time it takes to lift a finger from the key, idles at a fraction of Obsidian's memory, and never touches a file you did not edit.

## 2. Goals and non-goals

### Goals

- **Obsidian-compatible on disk.** A vault is a folder of `.md` files. nib reads `[[wikilinks]]`, `![[embeds]]`, `#tags`, YAML frontmatter, callouts and task lists the way Obsidian writes them, and writes links back in Obsidian's default "shortest unique path" form. `.obsidian/` is ignored and never modified. Switching back is `cd vault && obsidian`.
- **Fast.** Cold start to an editable note in well under 200 ms on a 3 000-note vault; a 10 000-line note opens in one frame; typing has no perceptible latency; idle RSS a third of Obsidian's. Numbers in §21.
- **Minimal.** One window: files column, editor, optional right panel (backlinks/outline), status line. No menu bar, no ribbon, no settings UI (settings are a TOML file you edit in nib itself), no WebKit, no libadwaita, no plugin system.
- **Keyboard-first, mouse-tolerated.** Vim in both halves: Trek's motions over the files, GtkSourceView's `VimIMContext` in the editor. The mouse works for clicking files, links, checkboxes and resizing panes; nothing *requires* it.
- **Safe with your notes.** Atomic saves, trash before delete, rename rewrites links only after asking, a crash can never leave a half-written note, and nib never creates files inside the vault other than notes, attachments and daily notes you asked for.

### Non-goals (v1)

- Live preview / WYSIWYG / hybrid mode. nib has **source mode** and **reading mode**, toggled, optionally side by side. This is the single biggest simplification versus Obsidian and is deliberate.
- Plugins, community themes, sync, publish, canvas, graph view, PDF export, math rendering (LaTeX is shown as source), Mermaid, spellcheck, mobile, Windows/macOS.
- Multiple vaults open at once (one window = one vault; a second vault is a second instance via `--profile`).
- Editing or generating `.obsidian/` config; honouring Obsidian hotkeys, CSS snippets or plugin data.
- Desktop-file-manager features Trek has that notes do not need: volumes, GVFS/remote places, dual pane, icon/thumbnail views, properties dialog, "Open with" menus, `org.freedesktop.FileManager1`.

## 3. Obsidian compatibility contract

| Obsidian thing | nib behaviour |
|---|---|
| Vault = folder; notes = `*.md` anywhere below it | Same. Vault root is the argument, else the nearest ancestor of the opened file containing `.obsidian/` or `.nib/`, else the file's folder. |
| `.obsidian/` | Hidden from the files column even with hidden files on; never read, never written, never trashed. |
| `[[Note]]`, `[[folder/Note]]`, `[[Note\|alias]]`, `[[Note#Heading]]`, `[[Note#^block]]` | Parsed by pulldown-cmark's wikilink extension; resolved by §12's rules (Obsidian's "shortest path when possible"). |
| `![[image.png]]`, `![[image.png\|300]]`, `![[Note]]`, `![[Note#Heading]]` | Images render in reading view (width hint honoured). Note/heading embeds render inline, depth 1, no recursion. |
| `[text](Note.md)`, `[text](<Some Note.md>)` relative Markdown links | Resolved relative to the linking note; count as links for backlinks. |
| `#tag`, `#nested/tag`, frontmatter `tags:` (string or list) | Indexed; tag completion after `#`; `:tag name` lists notes. |
| Frontmatter `title`, `aliases`, `tags` | Read. Everything else is kept byte-for-byte opaque. nib never adds frontmatter. |
| `> [!note] Title` callouts | Reading view: blockquote with a bold title line; type word shown as a label; no icons. |
| `- [ ]` / `- [x]` tasks | Highlighted in source; rendered as toggles in reading view; `Ctrl+Enter` toggles in both. |
| Daily notes `YYYY-MM-DD.md` in a folder | Same default (`daily/` folder, `%Y-%m-%d` name), configurable; template optional. |
| Templates folder with `{{date}}`, `{{time}}`, `{{title}}` | Same three variables plus `{{date:FORMAT}}` with strftime. No Templater. |
| Attachments folder | Default `attachments/`; config `attachments.folder`. Pasted images go there as `Pasted image YYYYMMDDHHMMSS.png` (Obsidian's own naming). |
| New link format | Shortest unique basename, else vault-relative path, `.md` omitted. Config `links.format = "shortest" | "relative" | "absolute"`. |
| Line endings, trailing newline, tabs/spaces, BOM | Preserved on save exactly as read. |

## 4. Stack and crates

Rust 2024 edition, stable toolchain ≥ 1.92. GTK 4 ≥ 4.14 (developed against Fedora's current, 4.20+). GtkSourceView 5 ≥ 5.10. Wayland is the target; X11 works through XWayland and is not tested.

| Crate | Why | Rules |
|---|---|---|
| `gtk4` (features `v4_14`), `glib`, `gio`, `gdk4`, `pango` | The UI toolkit. | No libadwaita. No `.ui`/Blueprint files; widgets are built in code. |
| `sourceview5` (feature `v5_10`) | Editor view/buffer, Markdown language spec, style schemes, `VimIMContext`. | The only editor widget. No custom text engine. |
| `pulldown-cmark` (`ENABLE_WIKILINKS`, `TABLES`, `STRIKETHROUGH`, `TASKLISTS`, `FOOTNOTES`, `HEADING_ATTRIBUTES`, `YAML_STYLE_METADATA_BLOCKS`) | One parser for both the reading view and link/tag/heading extraction, so the index and the renderer can never disagree. | No regex-based link scraping. |
| `nucleo` | Fuzzy matching for the quick switcher, command palette, `z` finder and `[[` completion; fzf query syntax, multi-core. | Same crate Trek uses. |
| `ignore` | Walking the vault honouring `.gitignore`/`.ignore`, skipping `.git`, `.obsidian`, `.nib`, `.trash`. | Same crate Trek uses. |
| `grep-searcher`, `grep-regex` | Vault full-text search at ripgrep speed, in-process. | No shelling out to `rg`. |
| `notify` + `notify-debouncer-mini` | Recursive vault watch (inotify). | Debounce 150 ms; coalesce per path. |
| `serde`, `toml` | Config and per-vault state files. | Config errors are reported, never fatal. |
| `chrono` | Daily-note names, template variables, modified-time display. | Local time only. |
| `thiserror` (libs), `anyhow` (binary) | Errors. | See §22. |
| `async-channel` | Thread → main-context results. | Only channel type in the codebase. |
| `memchr`, `unicode-segmentation` | Fast scans; word count. | Optional; drop if unused. |

Forbidden: `webkit`, `libadwaita`, `relm4`, `gtk-rs-core` wrappers beyond the above, `tokio`, `regex` for link parsing, `serde_yaml` (frontmatter is hand-parsed, §6), any crate that pulls in a second async runtime.

Release profile: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `debug = 1` (crash log line numbers), `panic = "unwind"` (the panic hook must run). Strip in the release archive, not the profile.

## 5. Repository layout

```
nib/
├── Cargo.toml                 # workspace; shared [workspace.dependencies] and lints
├── crates/
│   ├── nib-core/              # pure logic, no I/O, no GTK
│   ├── nib-vault/             # files, watching, index I/O, search, trash; no GTK
│   └── nib/                   # the GTK 4 application
├── data/
│   ├── io.github.codedsleep.Nib.desktop
│   ├── io.github.codedsleep.Nib.svg
│   ├── nib.1.scd              # man page source (scdoc)
│   └── style-schemes/nib-nord.xml, nib-light.xml
├── scripts/
│   ├── check.sh               # fmt --check, clippy -D warnings, tests, smoke start
│   ├── mkfixture.sh           # builds ~/.cache/nib-fixture (an Obsidian-shaped vault)
│   ├── dev-run.sh             # nib on the fixture with its own XDG dirs
│   ├── install.sh             # build release, install to ~/.local
│   └── agent/checks/          # headless key-driven UI checks (Trek pattern)
├── .planning/SCOPE.md
├── .github/workflows/         # check on push; release archive on v* tag
├── README.md, LICENSE (MIT), install.sh (curl | sh, same contract as Trek's)
```

**Crate boundaries are hard.** `nib-core` has no `std::fs`, no `gio`, and is fully unit-testable. `nib-vault` owns every byte read or written and every thread; it exposes plain structs and `async-channel` receivers, never GTK types. `nib` owns widgets, actions, the main context and nothing that could be tested without a display.

## 6. Domain model (`nib-core`)

```rust
pub struct NoteId(Arc<str>);          // vault-relative path, forward slashes, no ".md"
pub struct Vault { root: PathBuf }     // the one piece of I/O-adjacent data in core: a path value

pub struct NoteMeta {
    id: NoteId,
    title: String,                     // frontmatter title, else file stem
    aliases: Vec<String>,
    tags: Vec<Tag>,                    // from body and frontmatter, deduped, lowercase, nested by '/'
    headings: Vec<Heading>,            // (level, text, slug, line)
    links: Vec<Link>,                  // every outgoing link, resolved or not
    blocks: Vec<(String, u32)>,        // ^block-id → line
    mtime: SystemTime,
    size: u64,
}

pub struct Link {
    raw: String,                       // text between [[ ]] or the href, as written
    target: LinkTarget,                // Note { id, heading: Option<String>, block: Option<String> } | Unresolved(String) | External(Url) | Attachment(path)
    alias: Option<String>,
    embed: bool,
    line: u32, col: u32,               // byte offset too, for rename rewriting
}

pub struct Index {
    notes: HashMap<NoteId, NoteMeta>,
    by_stem: HashMap<String, Vec<NoteId>>,   // lowercase file stem → candidates (resolution)
    by_alias: HashMap<String, Vec<NoteId>>,
    backlinks: HashMap<NoteId, Vec<(NoteId, u32)>>,   // target → (source, line)
    tags: BTreeMap<Tag, Vec<NoteId>>,
    unresolved: BTreeMap<String, Vec<NoteId>>,
}
```

Core functions (all pure, all tested):

- `parse_note(id, text) -> NoteMeta` — one pulldown-cmark pass with the options in §4; also returns the frontmatter span so the editor can fold/dim it later.
- `frontmatter::parse(text) -> Option<Frontmatter { title, aliases, tags, span }>` — hand-rolled: a leading `---` block; keys `title`, `aliases`, `tags`; values as scalar, `[a, b]` flow list, or `- a` block list. Anything else is ignored, never rejected. Not a YAML parser; the test suite pins the exact subset.
- `resolve(index, from: &NoteId, raw: &str) -> LinkTarget` — §12 rules.
- `shortest_link(index, target: &NoteId) -> String` — the text to write for a link to `target` under the configured link format.
- `rename_plan(index, old: &NoteId, new: &NoteId) -> Vec<Edit { note, byte_range, replacement }>` — every link text that must change, preserving `|alias`, `#heading`, `^block` and embed `!`.
- `template::expand(text, vars: &TemplateVars) -> String`.
- `daily::path(cfg, date) -> NoteId`, `daily::offset(cfg, from, days) -> NoteId`.
- `fuzzy::Query` — a thin wrapper that turns a string into nucleo's pattern with fzf syntax.
- `settings::Config` + `Config::parse(toml) -> (Config, Vec<Warning>)` — unknown keys warn, bad values fall back to defaults and warn.
- `cli::parse(args) -> Invocation` — §18.

## 7. Window and layout

```
┌──────────────┬────────────────────────────────────────────┬──────────────┐
│ files        │ [tab] [tab] [tab]                          │ Backlinks /  │
│ (F9)         │                                            │ Outline (F8) │
│              │  editor (source)  │  reading view (Alt+⇧E) │              │
│              │                   │  (optional split)      │              │
├──────────────┴────────────────────────────────────────────┴──────────────┤
│ NOR  :w                 daily/2026-10-16.md  ·  412 words  ·  18:3  ·  3 041 notes │
└──────────────────────────────────────────────────────────────────────────┘
```

- `gtk::ApplicationWindow` > `gtk::Box(vertical)` > [`gtk::Paned(h)` { files | `gtk::Paned(h)` { centre | right } }, status line].
- Default proportions: files 1 : centre 4 : right 1.5. Pane positions persist per vault (§19).
- The files column and right panel start **visible**; the reading split starts hidden. `F9`/`F8` toggle; the toggles persist.
- No header bar, no menu bar, no toolbar. The window title is `note title — vault name — nib`. CSD is whatever GTK gives under Hyprland (none); do not add a `HeaderBar` to get a close button.
- Dialogs: only two kinds exist — a yes/no question and a one-line text entry — both as `gtk::Window` transients (float under Hyprland without rules) and both answerable with `y`/`Enter`, `n`/`Esc`. Everything else is an inline popover (completion, switcher, palette, search) or a status-line notice.
- The status line (one `gtk::Box` of labels, monospace, one row): `[mode/vim command bar] [note path, ● if unsaved] [word count] [line:col] [index status | notice]`. Notices replace the right slot for 4 s and are also logged.

## 8. The files column (Trek-derived)

A single `gtk::ListView` over a `gio::ListStore` of `glib::BoxedAnyObject`-wrapped entries, with a `SignalListItemFactory` whose rows are one `gtk::Label` (name, folders suffixed `/`) and one right-aligned dim `gtk::Label` (linemode: nothing by default, `,m` shows modified age like `3h`, `2d`). No icons. One row = one line of text; the list must scroll 20 000 rows without a hitch (ListView is virtualised; the factory must not allocate per bind beyond setting label text).

A breadcrumb label above the list shows the vault-relative folder (`~` for root). A parent column (Trek's left column) is **off** by default and toggled with `F7`; when on it is a second identical ListView, one part wide to the files column's three.

**Listing model, from Trek:** a folder is read once on `nib-vault`'s listing thread (`std::fs::read_dir` + metadata, sorted in `nib-core`), delivered whole over the channel, and kept alive in an LRU of the last 12 folders the column has shown — each still watched, so `h` then `l` reads nothing. The vault watcher (§16) feeds the same folder cache. `F5` rereads from disk whatever is cached.

Sort: natural name order, folders first (`,n`); modified time newest first (`,m`); `,r` reverses the current order. Hidden files (dotfiles) off by default, `.` toggles; `.obsidian`, `.nib`, `.git`, `.trash` never show. `files.only_markdown = false` by default — attachments are real and you want to trash them.

**Cursor and selection are different things** (Trek's rule): moving the cursor selects nothing; a command acts on the selection, or on the row under the cursor when nothing is selected.

Keys, active only while the column has focus (full table in §17): `j k gg G Ctrl+U Ctrl+D` move; `h l` parent / enter-or-open; `o`/`Enter` open in the current tab, `O` in a new tab; `Space v Esc Ctrl+A` selection; `y x p P` yank/cut/paste/paste-over; `d D` trash/delete; `a` create; `r` rename; `/ n N` find in list; `f` filter; `z` fuzzy finder below this folder; `.` hidden; `,…` sort; `c…` copy path/name/wikilink; `g…` go to root/daily/attachments/templates/recent.

**Clipboard interop:** yank and cut put `text/uri-list` on the compositor clipboard with a `gdk::FileList` content provider, plus the `x-special/gnome-copied-files` `copy\n`/`cut\n` form, exactly as Trek does — so files yanked in nib paste in Trek, and files yanked in Trek paste in nib with `p`. `cl` yanks a wikilink as plain text instead.

**Operations** (`nib-vault::ops`, each on a blocking thread, each reporting progress and the result over the channel): copy, move, trash (`gio::File::trash`), delete (recursive, own walk, refuses at a mount point and asks at a symlinked folder), rename, create. Conflicts ask once per operation (skip / replace / rename with ` (1)` suffix). There is no progress window; the status line shows `copying 3/12…` and `w` is not needed.

**Rename is link-aware.** Renaming or moving a `.md` note (or a folder of notes) computes `rename_plan` and asks: `Update 14 links in 9 notes? [y/n]`. On `y` the edits are applied with atomic writes, oldest-byte-offset-last per file, and the notice says how many were rewritten. Open buffers for affected notes are patched in place (not reloaded) so undo history survives. Config `links.rewrite_on_rename = "ask" | "always" | "never"`.

**Create:** `a` opens the entry; a name ending `/` makes a folder; a name without an extension gets `.md` and opens the new note in the editor with the cursor in it. `A` is Trek's bulk-create and is not available.

**Opening non-notes:** images open in a reading-view tab showing the picture; everything else is launched with `gio::AppInfo::launch_default_for_uri` and a notice.

**The `z` finder** is Trek's: nucleo over everything below the current folder (vault-relative paths), `ignore`-walked on a thread, matched on all cores, `Enter` enters a folder or opens a note, `Esc` closes. It reuses the quick switcher's popover widget (§13) with a different source.

## 9. The editor

`sourceview5::View` in a `gtk::ScrolledWindow`, one per tab, over a `sourceview5::Buffer` with the `markdown` language and the configured style scheme.

Settings on the view, all from config with these defaults: wrap `WordChar`; line numbers off; highlight current line off; right margin off; tab width 4, insert spaces off (Markdown lists indent with the note's own convention — nib detects tabs vs spaces from the first indented line and keeps it); auto-indent on; smart backspace on; smart home/end `Before`; monospace font from GSettings `org.gnome.desktop.interface monospace-font-name`, fallback `Monospace 11`, override `editor.font`; top/bottom margin 12 px, left/right margin from `editor.margin` (default 24 px); maximum line width `editor.max_width` (default 0 = none; when set, the view is centred in a `gtk::Box` with that width request, the Obsidian "readable line length" feature).

Editing behaviours implemented in `nib` (each a small function in `editor/behaviours.rs`, each with a headless check):

- **List continuation.** `Enter` on a `- `, `* `, `1. `, `- [ ] ` line inserts the next marker (numbers increment); `Enter` on an empty marker line removes the marker instead. Honours the line's indentation. Only when vim is in insert mode or vim is off (nib cannot ask VimIMContext its mode; it keys off `command-bar-text == "-- INSERT --"`, see §10).
- **Auto-pair** `[[` → `[[]]` with the cursor inside, `**`, `` ` ``, `(`, `[`, `"` — config `editor.auto_pair` (default: only `[[` and `` ` ``; the rest off, because vim users have their own habits).
- **Checkbox toggle** `Ctrl+Enter`: on a task line flips `[ ]`↔`[x]`; on a plain list line adds `[ ]`; otherwise does nothing. Also `:check`.
- **Follow link** `Alt+Enter` (also `:follow`): the link whose span contains the cursor. Wikilink → open the note (create it if unresolved, in the configured new-note folder, after a notice — Obsidian's behaviour); `#heading` → scroll to it; `^block` → scroll to the line; external URL → `gio::AppInfo::launch_default_for_uri`; attachment → open as §8 says. `Ctrl+click` does the same with the mouse.
- **Heading cycle** `Ctrl+Shift+Up/Down`: promote/demote the heading on the current line (`#`→`##`…, never past `######`, `#` removed on promote).
- **Completion** (§12): a `gtk::Popover` with a `ListView`, opened by typing `[[` (notes + aliases + headings after `#`), `#` at a word boundary (tags), `![[` (attachments first, then notes), or `Ctrl+Space` on a partial link; nucleo-ranked; `Tab`/`Enter` accept, `Esc` dismisses, `Up/Down`/`Ctrl+N`/`Ctrl+P` move. Accepting writes the link in the configured format and, for `[[`, places the cursor after the closing `]]`. This is nib's own popover, not a `CompletionProvider` subclass: it is 200 lines, fully controllable, and never lags the keystroke.
- **Paste image** (§14), **drop files** (§14).
- **Word count** in the status line recomputed on an idle callback after edits, debounced 300 ms, counting Unicode words outside code fences.

The buffer's undo is GtkSourceView's; nib never replaces or clears it except on `:e!`.

## 10. Vim

`sourceview5::VimIMContext`, attached per view via a `gtk::EventControllerKey` in the capture phase with `set_im_context(&vim)` and `vim.set_client_widget(&view)`. On when `editor.vim = true` (default). Off means a plain GtkSourceView with GTK's default text editing keys — every feature of the app must still be reachable (the `Alt`/`Ctrl` chords and the palette guarantee that).

- The status line's first slot is bound to the context's `command-bar-text` property and the second to `command-text` (the pending keys, like Trek's started-sequence display). `-- INSERT --` / `-- VISUAL --` / `:` text appear there as GtkSourceView emits them; nib shows `NOR` when the bar is empty.
- `execute-command` is connected and handles: `:w`, `:w!`, `:wq`, `:x`, `:q`, `:q!`, `:qa`, `:e {note}`, `:e!`, `:{number}`, and every nib command in §17. Unknown commands produce a status notice `not an editor command: …` and return `false`.
- `:w` saves; `:q` closes the tab (asks if modified); `:qa` closes the window (asks once listing the modified notes).
- nib binds **no plain normal-mode letters** of its own in the editor; VimIMContext owns them. Everything nib adds is an `Alt`/`Ctrl`/`F` chord that vim does not claim, or a `:` command. `Ctrl+O`, `Ctrl+I`, `Ctrl+D`, `Ctrl+U`, `Ctrl+F`, `Ctrl+B`, `Ctrl+E`, `Ctrl+Y`, `Ctrl+R`, `Ctrl+V`, `Ctrl+W`, `Ctrl+A`, `Ctrl+X`, `Ctrl+N`, `Ctrl+P`, `Ctrl+G` are vim's and are never window accelerators. `Ctrl+S`, `Ctrl+Enter`, `Ctrl+Space`, `Ctrl+Q` are nib's.
- Known limits of `VimIMContext` are documented in `README.md` under "Vim" as they are discovered (no `:map`, no registers UI, no `gf`, no `Ctrl+W` splits); nib does not reimplement vim around them. `gf` is `Alt+Enter`.

## 11. Reading view

A `gtk::TextView` (not Source) with a `TextTagTable` built once at start-up and shared across tabs. Rendering is `render(markdown, ctx) -> Vec<Op>` in `nib-core::render` (pure; `Op` is `Text{str, tags}`, `Anchor{Image|Embed|Task|Rule}`, `Newline`), applied to the buffer in one pass in `nib`. Rendering a 10 000-line note must take under 50 ms including buffer insertion; the ops vector is built first so the buffer sees one `begin_user_action`/`end_user_action` and the view relayouts once.

Tags (colours taken from the active style scheme's `def:heading`, `def:emphasis`, `def:string`, `def:comment`, `text` styles so the reading view matches the source view): `h1`–`h6` (scale, weight, margin above), `em`, `strong`, `strike`, `code` (monospace, subtle background), `codeblock` (monospace, left margin, background, preserves whitespace, language shown as a dim label line; no syntax highlighting in v1), `quote` (left margin + bar via `left-margin` and a dim foreground), `callout-title` (bold), `list` with `indent` tags per level and bullet glyphs `•` `◦` `▪`, ordered numbers as text, `task` (`☐`/`☑` glyphs; clicking flips the source line and re-renders that paragraph only), `link` (underline; clickable; wikilinks resolved — unresolved links drawn dim with a dashed underline, click creates), `hr` (a `gtk::Separator` at an anchor), `table` (rendered as monospace, column-padded text in a `codeblock`-styled run — a real grid is deferred), `footnote` (superscript number; definitions listed at the end), `frontmatter` (not rendered; a single dim line `— 3 properties —` at the top that expands on click to show the raw block).

Images: `gtk::Picture` at a `TextChildAnchor`, loaded via `gdk_pixbuf::Pixbuf::from_file_at_scale` to the view's width (or the `|300` hint) on `nib-vault`'s thread, placeholder box until it lands. Only vault-local files; remote `![](https://…)` is shown as its link text.

Embeds `![[Note]]`: the target note rendered inline in a `quote`-styled region with a title line, depth 1 (an embed inside an embed is shown as a link). `![[Note#Heading]]` renders that section only.

Mode switching: `Alt+E` toggles the tab between source and reading (the reading view replaces the source view in the same slot; scroll position is mapped by line ratio — not exact, good enough). `Alt+Shift+E` shows both side by side in a `gtk::Paned`; the reading side re-renders on a 400 ms idle after edits and follows the source cursor's line on `Ctrl+L`-free "scroll-sync by ratio". Reading view is read-only; `i` or `Alt+E` returns to source. In reading view, `j`/`k`/`gg`/`G`/`Ctrl+D`/`Ctrl+U` scroll (nib's own handler — there is no vim context here) and `Enter` follows the focused link.

## 12. Links, index, backlinks

**Index build.** On vault open, `nib-vault::index` walks with `ignore`, reads every `.md` on a `rayon`-free thread pool of `available_parallelism()` std threads (chunked work queue; no rayon dependency), parses with `nib-core::parse_note`, and sends `IndexEvent::{Progress(n, total), Ready(Index)}`. 3 000 notes must index in under 400 ms on a laptop; 30 000 in under 4 s; the UI never waits — the editor opens before the index is ready and link features switch on when it lands. The status line shows `indexing 1 204/3 041` meanwhile. No on-disk index cache in v1 (measure first; add only if the budget fails on real vaults).

**Incremental updates.** The watcher (§16) and every save re-parse that one note and patch `Index` (remove old links/tags/backlinks, insert new). A rename/move patches ids. The patch must be O(size of that note), not a rebuild.

**Resolution** (`nib-core::resolve`), in order, the first hit wins:
1. `raw` contains `/`: exact vault-relative path match (with or without `.md`), case-sensitive then case-insensitive.
2. Exact file stem match, case-insensitive; if several, prefer the one in the linking note's folder, then the shortest path, then alphabetical (Obsidian's tie-break).
3. Alias match, case-insensitive, same tie-break.
4. `Unresolved(raw)`.

`#heading` matches by slug (lowercase, non-alphanumerics collapsed to `-`) against the target's headings; `^block` by id. A heading/block that does not exist still resolves the note and shows a notice on follow.

**Writing links:** `shortest_link` → file stem if `by_stem[stem].len() == 1`, else vault-relative path without `.md`. Config can force relative or absolute.

**Backlinks panel** (right panel, `F8`, tab 1): for the current note, a `ListView` grouped by source note, each row the source title and the line containing the link (trimmed, the link text bolded). `Enter`/click opens the source at that line. Updates when the index patches or the tab switches. A second section lists **unresolved links in this note** (Obsidian's "unlinked" is out of scope).

**Outline** (right panel, tab 2, `Tab` switches): the headings of the current note as an indented list; `Enter` scrolls the editor there; follows the cursor's section with a highlight. Rebuilt from the buffer on the same 300 ms debounce as word count, not from the index, so it is live while typing.

**Tags:** `:tag` with no argument opens the switcher over tags (count beside each); choosing one lists its notes in the switcher. `#` completion in the editor.

## 13. Quick switcher, command palette, vault search

All three are the same widget: `Finder` — a `gtk::Popover` (positioned top-centre of the window, 60 % width, max 14 rows) with a `gtk::Entry` and a `ListView`, fed by a `FinderSource` trait object that supplies candidates and renders rows, ranked by nucleo on all cores with results streamed as they arrive. `Up`/`Down`/`Ctrl+N`/`Ctrl+P`/`Tab`/`Shift+Tab`, `Enter` accept, `Shift+Enter` accept-in-new-tab, `Esc` close. Empty query shows the source's default order.

- **Quick switcher** `Alt+O`, `:o {query}`: candidates are every note's title, aliases (shown as `alias → title`) and path; default order is most recently opened. A query with no match offers a first row `Create "query"` which creates `query.md` in `notes.new_folder` (default: vault root; `"current"` = the folder of the current note; `"daily"` etc.) and opens it. This is the one keystroke that must feel instant: the candidate list is built from the index once and kept; typing never re-reads disk.
- **Command palette** `Alt+P`, `:`-less: every `gio::SimpleAction` the app registers, with its label and accelerator hint; the same registry drives the key table (`F1`). Adding a feature means adding an action in one place.
- **Vault search** `Alt+F`, `:find {query}`: `grep-searcher` over the vault on a thread, smart-case literal by default, `r:` prefix for regex, `-` prefix on a word excludes a file-path substring; results are `path:line: context` rows grouped by note, streamed, capped at 2 000 with a `more…` row; `Enter` opens at the line. The search is cancelled when the query changes. Searching 3 000 notes for a common word must return the first page in under 100 ms.

## 14. Daily notes, templates, attachments

**Daily** `Alt+D`, `:daily`, `:daily +1`, `:daily -1`, `:daily 2026-10-01`: opens (creating if needed, from `daily.template` if set) `daily.folder/strftime(daily.format)`. Creation never overwrites. `gd` in the files column goes to the folder.

**Templates** `:template {name}` / `Alt+T` → finder over `templates.folder/*.md`: inserts the expanded template at the cursor (or replaces an empty buffer). Variables: `{{date}}` (`daily.format`), `{{date:%d %B %Y}}`, `{{time}}` (`%H:%M`), `{{title}}` (the note's title). Unknown `{{x}}` is left as is. No code execution, ever.

**Attachments:**
- `Ctrl+V` with an image on the clipboard (`gdk::Texture` offered): saved as PNG to `attachments.folder` as `Pasted image YYYYMMDDHHMMSS.png`, `![[name.png]]` inserted. `Ctrl+V` with text is vim's/GTK's normal paste. The check order is image first, then text.
- Dropping files on the editor: each is copied into `attachments.folder` (asking before replacing an existing name) and an `![[name]]` (images) or `[[name]]` link is inserted per file. Dropping a `.md` file from inside the vault inserts a link without copying.
- `:attach` opens a file chooser (`gtk::FileDialog`) for the same.
- Attachments appear in the files column like any file; trashing one that is still linked shows a notice with the count, not a refusal.

## 15. Tabs, history, session

- A tab = one note, one `Buffer`, one `View`, one optional reading view. The tab bar is a row of `gtk::ToggleButton`s in a `gtk::Box` above the editor (no `Notebook`), title ellipsised, `●` when modified, middle-click closes. `Alt+T` new empty tab (untitled, becomes a note on first `:w` via a name prompt), `Alt+W` close, `Alt+[`/`Alt+]` previous/next, `Alt+1…9` are **not** tabs (they focus panes, §17); tab N is `:tab N`. Opening a note already open in a tab switches to it instead of duplicating. Max 32 tabs; the oldest unmodified is closed when exceeded.
- **History** per tab: `Alt+Left`/`Alt+Right` go back/forward through notes opened in that tab (cursor position remembered), like a browser tab.
- **Recent:** the switcher's default order; `gr` in the files column lists recent notes there too.
- **Session:** on close and every 30 s while dirty, nib writes `~/.local/state/nib/<vault-key>/session.toml` (`vault-key` = sha256 of the canonical root path, first 16 hex chars): open tabs, their notes and cursor lines, the active tab, pane sizes, panel visibility, files-column folder and cursor, sort mode. Restored on next open of that vault. Modified-but-unsaved buffers are **not** persisted — with autosave on they cannot exist for more than a second.

## 16. Saving, watching, external changes

- **Save** = write to `.{stem}.nib-tmp` in the same directory, `fsync`, copy the original's permissions, `rename` over the original. Never truncate-in-place. A crash leaves at most a `.nib-tmp` file beside the note, and nib deletes stale ones it finds on open (they match `.*.nib-tmp` and are older than 1 min). Bytes written are exactly the buffer's text with the original line endings and trailing-newline state restored (nib normalises to `\n` in the buffer and remembers).
- **Autosave** `editor.autosave = true` (default): 1 000 ms after the last edit, on tab switch, on focus-out of the window, on `Alt+E`, and before any operation that reads the file from disk (rename plan, search). `:w` saves now. With autosave off the tab shows `●` and `:q` asks.
- **Watcher:** one recursive `notify` watcher on the vault root, debounced; events are mapped to `Changed(path)`, `Created`, `Removed`, `Renamed(from, to)` and fanned to the folder cache (§8), the index (§12) and open tabs. Writes nib made itself are recognised by path + a generation counter and ignored. If inotify watches run out (`ENOSPC`), nib shows one notice with the `sysctl` line to fix it and falls back to re-listing the visible folder on focus-in.
- **External change to an open note:** unmodified buffer → reloaded in place, cursor kept by line, notice `reloaded daily/2026-10-16.md`. Modified buffer → notice `changed on disk — :e! to reload, :w to overwrite`; nothing automatic. Removed on disk → notice; the buffer stays and `:w` recreates the file.

## 17. Keybindings and commands

Keys are commands only while the files column has the keyboard; in the editor, vim has them; in any entry or popover, every key is text. The `Alt`/`Ctrl`/`F` chords below are window-level `gtk::ShortcutController` actions in the bubble phase so an entry can eat them first. `F1`/`~` (files column) lists everything with a filter entry, built from the action registry.

### Window (any focus)

| Key | Action |
|---|---|
| `Alt+O` | quick switcher |
| `Alt+P` | command palette |
| `Alt+F` | vault search |
| `Alt+N` | new note (prompt for title; created in `notes.new_folder`) |
| `Alt+D` | today's daily note |
| `Alt+T` | new tab · `Alt+Shift+T` insert template |
| `Alt+W`, `Alt+[`, `Alt+]` | close tab, previous, next |
| `Alt+Left`, `Alt+Right` | history back, forward |
| `Alt+1`, `Alt+2`, `Alt+3` | focus files column, editor, right panel |
| `Alt+E`, `Alt+Shift+E` | toggle source/reading; toggle split |
| `Alt+B` | focus backlinks (opens the panel if hidden) |
| `F7`, `F8`, `F9` | parent column, right panel, files column |
| `F5` | reload folder listing from disk |
| `F11` | fullscreen |
| `Ctrl+S` | save current note |
| `Ctrl+Q` | quit (asks if anything is modified and autosave is off) |
| `Esc` | close any popover; in the files column, clear selection and filter |

### Files column

| Key | Action |
|---|---|
| `j` `k`, `gg` `G`, `Ctrl+U` `Ctrl+D`, `Ctrl+B` `Ctrl+F` | move; single steps wrap |
| `h`, `l`/`Enter`/`o`, `O` | parent; enter folder or open note; open in new tab |
| `H` `L` | folder history back, forward |
| `Space`, `v`, `Ctrl+A`, `Ctrl+R`, `Esc` | toggle, visual mode, select all, invert, clear |
| `y` `x` `p` `P`, `Y` | yank, cut, paste, paste-over; unyank |
| `d`, `D` | trash; delete permanently (asks) |
| `a` | create (`name/` folder, `name` → `name.md`) |
| `r` | rename (link-aware, §8) |
| `/` `?` `n` `N`, `f` | find in list; filter (live, `Esc` clears) |
| `z` | fuzzy finder below this folder |
| `.` | hidden files |
| `,n` `,m` `,r` | sort by name, modified; reverse |
| `cp` `cn` `cl` `cr` | copy absolute path, name, `[[wikilink]]`, vault-relative path |
| `gh` `gd` `ga` `gt` `gr` | go to vault root, daily folder, attachments, templates, recent notes |
| `~`, `F1` | key list |
| `Tab` | focus the editor (`Alt+2` also) |

### Editor (chords; everything else is vim)

| Key | Action |
|---|---|
| `Ctrl+S` | save |
| `Alt+Enter`, `Ctrl+click` | follow link under cursor (creates unresolved notes) |
| `Ctrl+Enter` | toggle task checkbox |
| `Ctrl+Space` | open completion for the link/tag at the cursor |
| `Ctrl+Shift+Up/Down` | promote/demote heading |
| `Ctrl+V` | paste (image → attachment, else text) |
| `Tab`, `Shift+Tab` in a list line (insert mode / vim off) | indent, dedent the item |

### `:` commands (vim command bar; also the palette with the `:` stripped)

`w` `w!` `wq` `x` `q` `q!` `qa` `e {note}` `e!` `{N}` · `o [query]` `new [title]` `daily [±N|date]` `find {query}` `tag [name]` · `follow` `check` `backlinks` `outline` `files` `read` `edit` `split` · `rename {new-name}` `trash` `template [name]` `attach` · `tab [N|new|close|next|prev]` · `config` (opens `nib.toml` in a tab; it is reloaded on save, warnings in the status line) `reindex` `help [key]` `version`.

## 18. CLI, single instance, desktop integration

```
nib                      open the last vault (or the fixture-less first-run prompt: a folder chooser)
nib VAULT_DIR            open that vault
nib NOTE.md              open the vault that contains it, at that note (creates nothing)
nib --new "Title"        open the last vault and create+open a note
nib --daily              open the last vault at today's daily note
nib --vault DIR …        combine with the above
nib --profile NAME       separate instance and state (a second vault open at once)
nib --version, --help
```

`gio::Application` with `HANDLES_OPEN | HANDLES_COMMAND_LINE`; one instance per profile, so `nib foo.md` from a terminal raises the existing window and opens the note in it (the Hyprland activation token is passed so the window takes focus). App id `io.github.codedsleep.Nib` (`.p_NAME` with a profile) for window rules; `bind = $mainMod, N, exec, nib` is the README's suggested binding. `.desktop` file with `MimeType=text/markdown;` so "Open with nib" works from Trek. No D-Bus services beyond GApplication's own.

## 19. Config and state files

`~/.config/nib/nib.toml` (profile: `~/.config/nib/PROFILE/nib.toml`), created with commented defaults on first run. Edited in nib with `:config`; reloaded on save; every change applies live where it can (fonts, scheme, wrap, autosave, margins) and the rest at next vault open (noted in the file's comments).

```toml
[vault]
last = "/home/percy/notes"       # written by nib
new_folder = ""                  # "" = root; "current"; or a folder
attachments = "attachments"
daily_folder = "daily"
daily_format = "%Y-%m-%d"
daily_template = ""              # "" = none; a path under templates
templates = "templates"

[editor]
vim = true
autosave = true
font = ""                        # "" = GSettings monospace font
line_numbers = false
wrap = true
max_width = 0                    # 0 = none; e.g. 760
margin = 24
auto_pair = ["[[", "`"]
style_scheme = "nib-nord"        # or any GtkSourceView scheme id
reading_font = ""                # "" = GSettings interface font, +1pt

[links]
format = "shortest"              # shortest | relative | absolute
rewrite_on_rename = "ask"        # ask | always | never

[files]
only_markdown = false
show_hidden = false
sort = "name"                    # name | modified
show_parent_column = false
linemode = "none"                # none | modified
```

Per-vault state (`~/.local/state/nib/<vault-key>/session.toml`, §15) is nib's; the vault itself gets nothing. `~/.local/state/nib/crash.log` on panic (§22). Cache: none in v1.

## 20. Theme and fonts

Dark by default, in the desktop GTK theme's colours (`gtk-application-prefer-dark-theme = true`); `GTK_THEME=Adwaita:light nib` for a light run. nib ships one CSS file (`nib.css`, loaded via `gtk::CssProvider` at `STYLE_PROVIDER_PRIORITY_APPLICATION`) that only sets: status-line font/padding, tab-bar look, files-column row padding and dim colour, finder popover sizing, selected/cursor row colours derived from `@theme_selected_bg_color`. No theme is bundled; Trek's GTK 3-palette carry-over is **not** ported (measure whether the desktop's theme has a GTK 4 part first; it is listed in §27).

Style schemes: `nib-nord` (default) and `nib-light`, installed by `install.sh` into `~/.local/share/gtksourceview-5/styles/` and also added at runtime via `StyleSchemeManager::append_search_path` from the binary's data dir so a source build works without installing. Any other installed scheme id works via config. The reading view takes its colours from the scheme (§11), so one setting themes both views.

## 21. Performance budget and how to meet it

Measured on a 3 000-note / 40 MB fixture (`scripts/mkfixture.sh` generates it with realistic link density) on a 2020-class x86_64 laptop, release build, warm cache, `NIB_TRACE=1` printing each number:

| Metric | Budget | How |
|---|---|---|
| Process start → window mapped with the last note editable | ≤ 150 ms | Nothing but config, session and the one note are read before `present()`. Index, folder listing, watcher start on threads after the first frame (`glib::idle_add_local_once` after `present`). GtkSourceView language/scheme managers are constructed once, lazily. |
| Index ready (3 000 notes) | ≤ 400 ms after start | Parallel parse, `parse_note` ≤ 100 µs for a 5 KB note. |
| Open a note already indexed, 10 000 lines | ≤ 16 ms to first paint | `Buffer::set_text` once; highlighting is GtkSourceView's incremental. No per-line work in nib. |
| Keystroke → glyph | no nib work on the key path beyond the vim IM context and the `[[` detector (≤ 50 µs) | Word count, outline, autosave, reading re-render are all idle-debounced. |
| Quick switcher: popover visible | ≤ 30 ms; first ranked results ≤ 10 ms after a keystroke | Candidates pre-built; nucleo on all cores; the ListView shows at most 14 rows. |
| Vault search first page (common word) | ≤ 100 ms | `grep-searcher` with a memmap-less reader, `ignore` parallel walk, results streamed. |
| Reading view render, 10 000 lines | ≤ 50 ms | Ops vector then one insertion. |
| Idle RSS, vault open, 3 tabs | ≤ 90 MB | No cache of file contents beyond open buffers and the 12-folder LRU of names; `NoteMeta` strings interned via `Arc<str>` ids. |
| Files column, 20 000 entries folder | scrolls at 60 fps | Virtualised `ListView`; factory binds set two label texts and nothing else. |

Rules that keep the budget: no work on the main thread that can take more than 4 ms (the `NIB_TRACE` stall probe asserts this in dev runs, as Trek's does); every thread result crosses one `async-channel`; no `clone()` of note text to hand it between widgets (buffers own text, `nib-core` borrows `&str`); no reindex on save (patch); no polling anywhere.

## 22. Errors, logging, crash log

- `nib-core` and `nib-vault` return `thiserror` enums; `nib` converts them to notices (`anyhow` context for the message). An error never closes the window and never loses buffer text.
- `NIB_LOG=debug|info|warn` (default `warn`) prints to stderr via `env_logger`-free hand-rolled `log` sink (a 40-line `log::Log` impl; `log` crate is allowed). `NIB_TRACE=1` adds timings.
- A panic hook writes the message, backtrace, nib version, GTK/GtkSourceView versions and the vault key to `~/.local/state/nib/crash.log` (appending), then the default hook runs. Autosave means at most 1 s of typing is lost.

## 23. Conventions

- `cargo fmt` default; `cargo clippy --all-targets -- -D warnings` with `clippy::pedantic` enabled at the workspace level and the usual allows listed in `Cargo.toml` (`module_name_repetitions`, `must_use_candidate`, `missing_errors_doc`). No `unsafe`. `#![forbid(unsafe_code)]` in all three crates.
- No `unwrap()`/`expect()` outside tests except on invariants that cannot fail by construction, with the invariant in the `expect` message. GTK downcasts use `downcast_ref` and return early on `None`.
- GTK state lives in one `Rc<App>` with interior `RefCell` fields per concern (`tabs`, `files`, `index`, `settings`), borrowed for the shortest possible scope and never across an `await` or a signal emission. Signal handlers capture `Weak<App>` and upgrade.
- Threads: `std::thread` or `gio::spawn_blocking`; results via `async-channel` consumed in `glib::spawn_future_local`. Never block the main context; never touch a widget off it.
- Files: every path that crosses into `nib-vault` is absolute and canonical; `nib-core` sees `NoteId`s only.
- Tests: `nib-core` ≥ 90 % line coverage by `cargo llvm-cov` (not enforced in CI, reported); `nib-vault` integration tests on `tempdir` vaults; `nib` has the headless checks (§24). Fixtures live under `crates/*/tests/fixtures/` as real Obsidian-exported vault fragments.
- Commits: one slice per PR-sized commit series; `README.md` updated in the same commit as a user-visible change; `CHANGELOG.md` kept in Keep-a-Changelog form from 0.1.0.
- Doc comments on every `pub` item in `nib-core` and `nib-vault`; module-level `//!` saying what the module owns. No comments that restate code.

## 24. Testing and scripts

- `scripts/check.sh`: `cargo fmt --check`, clippy, `cargo test --workspace`, then builds release and runs `nib --version` and a 2-second headless start-up smoke (`weston --backend=headless` or `cage` if installed, else `GDK_BACKEND=broadway` with a throwaway daemon; skip with a warning if none is available) that asserts the window maps and the trace prints `start_to_map` under budget.
- `scripts/mkfixture.sh`: generates `~/.cache/nib-fixture/` — 3 000 notes across 40 folders with wikilinks (some ambiguous stems, some aliases, some unresolved), tags, frontmatter, tasks, callouts, embeds, 20 images, a `.obsidian/` with junk, a `daily/` month, three templates, and `big.md` (10 000 lines). Deterministic from a seed.
- `scripts/dev-run.sh`: runs nib on the fixture with `XDG_CONFIG_HOME`/`XDG_STATE_HOME` under `~/.cache/nib-dev`, `NIB_TRACE=1`, and `NIB_ALLOW_ROOTS=~/.cache/nib-fixture` (Trek's confinement, ported: every write refused outside the listed roots, so a dev session cannot touch your real vault).
- `scripts/agent/checks/*.sh`: headless, key-driven checks in Trek's pattern — each starts nib on a fresh fixture copy, sends key sequences through a tiny `--test-keys` hook (a debug-only `gio::Action` that injects key events; compiled out of release), then asserts on files on disk and on `--test-dump` (JSON of tab titles, cursor, files-column cursor, status text). Checks required by the slices: open-and-save round trip preserves bytes; `[[` completion inserts shortest link; rename rewrites links; trash from the column; daily note creation; quick-switcher create; reading-view task toggle writes `[x]`; external change reload; autosave within 1.5 s.
- CI: `check.sh` on push; on `v*` tag, build `x86_64` and `aarch64` release archives with `SHA256SUMS`, as Trek's workflow does.

## 25. Build slices (vertical; each ends green)

1. **Skeleton.** Workspace, three crates, `App` with window, `nib VAULT` opens the files column (listing thread + LRU, `j k h l Enter`), a tab with a GtkSourceView Markdown editor, `Ctrl+S` atomic save, status line with path/line:col, `nib-nord` scheme, config file read with defaults, `NIB_TRACE` start-to-map timing. *Done when:* a note round-trips byte-exact; start-to-map ≤ 150 ms on the fixture.
2. **Vim and commands.** `VimIMContext` wiring, status-line bar/text, `execute-command` router with `w q wq e e! {N}`, the action registry + `Alt+P` palette skeleton (plain list, no fuzzy yet), `F1` key list. *Done when:* `:wq` works; every action has a label and an accelerator hint.
3. **Index and links.** `parse_note`, frontmatter, `Index`, parallel build with progress, resolution + tests on the fixture's ambiguous cases, `Alt+Enter` follow (create on unresolved), `[[`/`#`/`![[` completion popover, backlinks + outline panel, incremental patch on save. *Done when:* every resolution test passes; completion feels instant on 3 000 notes.
4. **File manager parity.** Selection model, yank/cut/paste with Trek clipboard interop, trash/delete, create, link-aware rename with plan + prompt, find/filter, sort, hidden, `c…`/`g…` sequences, `F7` parent column, recursive watcher feeding the folder cache. *Done when:* the rename check and the trash check pass; files yanked in Trek paste in nib and vice versa.
5. **Finder.** Nucleo-backed `Finder` widget; quick switcher with create row and recent order; palette becomes fuzzy; `z` finder in the column; `:tag`. *Done when:* switcher budgets in §21 hold.
6. **Reading view.** `render` ops, tag table, images, embeds, tasks toggling, links, `Alt+E` toggle and `Alt+Shift+E` split with idle re-render, reading-mode scroll keys. *Done when:* `big.md` renders ≤ 50 ms; task toggle check passes.
7. **Vault search.** `grep-searcher` source for `Finder`, streaming, cancel, open-at-line. *Done when:* first page ≤ 100 ms on the fixture.
8. **Daily, templates, attachments.** `Alt+D`/`:daily ±N`, templates with variables, paste-image, drop-files, `:attach`. *Done when:* their checks pass; `Pasted image …png` matches Obsidian's name format.
9. **Tabs, history, session, watcher polish.** Tab bar, `Alt+T/W/[/]`, per-tab history, session save/restore, external-change handling, autosave triggers, `:config` live reload, `--new`/`--daily`/single-instance CLI. *Done when:* closing and reopening restores exactly; the external-change and autosave checks pass.
10. **Ship.** Man page, `install.sh` (curl | sh with checksum verification, `--prefix`, `--version`, `--uninstall`), `.desktop` + icon, README with the key tables of §17 and a "Coming from Obsidian" section, release workflow, a performance table in the README produced by `NIB_TRACE` on the fixture. *Done when:* `curl … | sh` installs a working nib on a clean Fedora.

## 26. Acceptance checklist

- [ ] Opens an unmodified Obsidian vault; after a week of use, `git status` in the vault shows only notes/attachments I created or edited and nothing under `.obsidian/`.
- [ ] Every §21 budget met on the fixture, numbers printed by `NIB_TRACE=1` and copied into the README.
- [ ] Every §17 key does what the table says; `F1` lists them from the action registry, not a hand-written table.
- [ ] Every `:` command in §17 exists.
- [ ] Vim off (`editor.vim = false`) leaves every feature reachable.
- [ ] Rename a note that 14 others link to → prompted → all 14 rewritten, aliases and headings preserved, open buffers patched with undo intact.
- [ ] Kill -9 during a save → the note is either the old bytes or the new bytes, never truncated; a stray `.nib-tmp` is cleaned on next open.
- [ ] Paste a screenshot from `grim`/`wl-copy` → attachment saved, `![[…]]` inserted, renders in reading view.
- [ ] Yank a file in Trek, `p` in nib's column, and the reverse.
- [ ] `scripts/check.sh` green; all headless checks green; `cargo clippy` clean with pedantic.
- [ ] Idle RSS with the fixture open under 90 MB.

## 27. Later, maybe (not v1; do not build unprompted)

Real table grid in reading view · syntax highlighting inside fenced blocks in reading view (reuse GtkSourceView's highlighter on a hidden buffer) · on-disk index cache if real vaults exceed the budget · GTK 3-theme palette carry-over from Trek · unlinked mentions · local graph (as a list, never a canvas) · a second editor split · `:map`-style custom chords · spellcheck via `libspelling` · folding · export to HTML via the same `render` ops.
