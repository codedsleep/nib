# nib

A keyboard-first Markdown notes app for Linux, built with Rust, GTK 4 and
GtkSourceView 5. Open an Obsidian vault unchanged: write with vim motions,
follow wikilinks, and find notes without leaving the keyboard.

- Trek-style files column with selection, copy/cut/paste, XDG trash and link-aware renames.
- Wikilink and tag completion, backlinks, outline, fuzzy switcher and streamed vault search.
- Source, reading and split views; local images, callouts, task toggles and note embeds.
- Fenced code highlighted in the language its fence names (```` ```rust ````, `~~~python`) in source view.
- Daily notes, templates, pasted screenshots, file attachments, tabs and per-vault sessions.
- Atomic saves that retain BOM and line endings, read-only previews for non-UTF-8 files,
  and external-change notices that protect modified buffers.

## Install

Requires Linux x86_64, GTK 4.14+, GtkSourceView 5.10+, and an authenticated
[GitHub CLI](https://cli.github.com/) with access to this private repository.
Release binaries are built on Ubuntu 24.04. Fedora runtime dependencies:

```sh
sudo dnf install gtk4 gtksourceview5 gh
gh auth login
gh api repos/codedsleep/markdown-trekfm-rust/contents/install.sh \
  --jq .content | base64 --decode > /tmp/nib-install.sh
bash /tmp/nib-install.sh -y --version v0.1.0
```

The installer downloads the release using `gh release download`, verifies its
SHA256SUMS entry, and installs the binary, desktop file, icon, man page,
style schemes and Markdown language into `~/.local`. Add `~/.local/bin` to PATH. Use `--prefix /ABS/DIR`
for another location or `--uninstall -y` to remove installed files while keeping
config, sessions and notes. Without `--version`, the latest release is used.
Noninteractive installation requires `-y`.

Releases currently contain x86_64 only. GitHub's ARM runners for private
repositories consume billed/included Actions minutes; there is no unconditional
free ARM runner, so ARM users build from source. See [GitHub runner billing](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-private-repositories).

```sh
sudo dnf install gcc pkgconf-pkg-config gtk4-devel gtksourceview5-devel scdoc
# Install Rust 1.92+ first, then in this checkout:
scripts/install.sh -y            # build and install locally
scripts/install.sh -y --uninstall
```

## Use

```sh
nib                             # last vault; folder chooser on first run
nib /path/to/vault
nib /path/to/vault/note.md        # forwards to the running instance
nib /path/to/vault/subfolder
nib --vault /path/to/vault --new 'A note'
nib --vault /path/to/vault --daily
nib --profile work /path/to/another/vault
nib --help
```

Hyprland binding: `bind = $mainMod, N, exec, nib`.

Edit configuration using `:config` (or the Alt+P palette). It is created at
`~/.config/nib/nib.toml`, or `~/.config/nib/PROFILE/nib.toml`. Fonts, scheme,
wrap, margins, vim and autosave apply on save; vault folders apply at next open.
Sessions and crash logs live under `~/.local/state/nib`, with separate profiles.
XDG_CONFIG_HOME and XDG_STATE_HOME override those locations.

`NIB_LOG=debug|info|warn|error|off` controls logging; `NIB_TRACE=1` prints timings.
`NIB_GTK_THEME` overrides the theme used for GTK 3 palette carry-over;
`GTK_THEME=Adwaita:light` selects a light GTK theme.
`NIB_ALLOW_ROOTS` enables write confinement when set, including when empty;
normal installed use leaves it unset. See `man nib` for details.

## Coming from Obsidian

Choose the existing vault folder. nib uses plain Markdown, shortest unique
wikilinks, frontmatter titles/aliases/tags, and the existing daily/templates/
attachments folders. It ignores `.obsidian/`, `.trash/`, `.git/` and `.nib/`;
settings and session files stay outside the vault. It does not sync or execute
plugins, fetch remote images, run templates, or launch executable attachments.
External links open only for HTTP, HTTPS and mailto.

Rename asks before rewriting references and keeps aliases and heading fragments.
Delete moves to XDG trash and refuses if trash is unavailable. An external edit
reloads an unmodified buffer; a modified buffer shows a notice and waits for
`:e!` (reload and discard edits) or `:w` (explicitly overwrite). Back up your vault
and review changes during the first week of use.

Navigation keeps unsaved edits when autosave is off, a save fails, or the note
changed externally; save with `:w` or open another tab. Closing asks before
discarding remaining edits. Config changes require an explicit save. Opening
past 32 tabs closes the least recently used clean tab; if every tab has edits,
save or close one first.

Copied and completed attachment links retain their folder when basenames collide.
The `links.format` setting applies to note and attachment links, including paths
relative to the note being edited.

Templates support `{{date}}`, `{{time}}`, `{{title}}` and `{{date:FORMAT}}`.
Daily notes default to `daily/YYYY-MM-DD.md`. Ctrl+V pastes a screenshot only
in insert mode or with vim off; `:paste-image` works in any mode.

## Vim

GtkSourceView's VimIMContext provides modes, motions, text objects, registers,
search and undo. nib owns its documented `:` commands. `:map` and `gf` are
unsupported; substitution (`:s/a/b/`) was unreliable with GtkSourceView 5.20.
Insert sessions form a single undo action. Set `editor.vim = false` for ordinary
editing; every nib action remains available through Alt+P or a chord.

Enter continues lists; Tab/Shift+Tab indent them. Finder Shift+Enter opens a
new tab. Reading mode supports `j k gg G Ctrl+D Ctrl+U`, Tab/Enter for links,
and `i` to return to source.

## Keys

Generated from the application registry by `scripts/generate-docs.py`. F1 shows
and filters the same actions in the app. Files-column keys apply in that column.

<!-- registry:start -->

### Window and editor

| Keys | Commands | Action |
|---|---|---|
| `Ctrl+S` | `:w`, `:w!`, `:write` | Save note |
| `Ctrl+Q` | `:qa`, `:qa!`, `:qall` | Quit |
| `Alt+T` | `:tabnew` | New tab |
| `Alt+W` | — | Close the tab |
| `Alt+[` | — | Previous tab |
| `Alt+]` | — | Next tab |
| `Alt+Left` | — | Back to the tab's previous note |
| `Alt+Right` | — | Forward to the tab's next note |
| `Alt+D` | `:daily` | Today's daily note |
| `Alt+N` | `:new` | New note |
| `Alt+Shift+T` | `:template` | Insert a template |
| `Alt+O` | `:o`, `:open` | Open a note (quick switcher) |
| `Alt+P` | — | Command palette |
| `Alt+F` | `:find` | Search the vault |
| `Alt+Enter` | `:follow` | Follow the link under the cursor |
| `Ctrl+Enter` | `:check` | Toggle the task checkbox |
| `Ctrl+Space` | — | Complete the link or tag at the cursor |
| `Ctrl+Shift+Up` | — | Promote the heading |
| `Ctrl+Shift+Down` | — | Demote the heading |
| `Alt+E` | `:read` | Reading view (toggle) |
| `Alt+Shift+E` | `:split` | Source and reading side by side |
| `F9` | `:files` | Show or hide the files column |
| `F8` | — | Show or hide the right panel |
| `F7` | — | Show or hide the parent column |
| `Alt+B` | `:backlinks` | Focus the backlinks |
| `F11` | — | Fullscreen |
| `F5` | — | Reload the folder listing from disk |
| `F1` | `:help` | List every key and command |
| `Alt+1` | — | Focus the files column |
| `Alt+2` | — | Focus the editor |
| `Alt+3` | — | Focus the right panel |

### Files column

| Keys | Commands | Action |
|---|---|---|
| `j`, `<Down>` | — | Cursor down |
| `k`, `<Up>` | — | Cursor up |
| `gg`, `<Home>` | — | First entry |
| `G`, `<End>` | — | Last entry |
| `<C-d>` | — | Half a page down |
| `<C-u>` | — | Half a page up |
| `<C-f>`, `<PageDown>` | — | A page down |
| `<C-b>`, `<PageUp>` | — | A page up |
| `h`, `<Left>`, `<BS>` | — | Parent folder |
| `l`, `<CR>`, `o`, `<Right>` | — | Enter the folder or open the file |
| `O` | — | Open in a new tab |
| `H` | — | Previous folder |
| `L` | — | Next folder |
| `<Space>` | — | Select or unselect, then step down |
| `v` | — | Visual selection mode |
| `<C-a>` | — | Select everything |
| `<C-r>` | — | Invert the selection |
| `<Esc>` | — | Clear the selection and the filter |
| `y` | — | Yank (copy) to the clipboard |
| `x` | — | Cut to the clipboard |
| `p` | — | Paste here |
| `P` | — | Paste here, replacing existing names |
| `Y` | — | Forget the yank |
| `d` | — | Move to the trash |
| `a` | — | Create a note (name/ for a folder) |
| `r` | — | Rename (rewriting links) |
| `/` | — | Find in the list |
| `?` | — | Find backwards |
| `n` | — | Next match |
| `N` | — | Previous match |
| `f` | — | Filter the list |
| `z` | — | Fuzzy-find below this folder |
| `.` | — | Show or hide hidden files |
| `,n` | — | Sort by name |
| `,m` | — | Sort by modified time, showing ages |
| `,r` | — | Reverse the order |
| `cp` | — | Copy the absolute path |
| `cn` | — | Copy the name |
| `cl` | — | Copy a [[wikilink]] |
| `cr` | — | Copy the vault-relative path |
| `gh` | — | Go to the vault root |
| `gd` | — | Go to the daily folder |
| `ga` | — | Go to the attachments folder |
| `gt` | — | Go to the templates folder |
| `gr` | — | Recent notes |
| `~` | — | List every key and command |
| `<Tab>` | — | Focus the editor |

### Commands and palette

| Keys | Commands | Action |
|---|---|---|
| — | `:wq`, `:x` | Save and close the tab |
| — | `:q`, `:quit` | Close the tab |
| — | `:q!` | Close the tab, dropping changes |
| — | `:e` | Open a note by name |
| — | `:e!` | Reload the note from disk, dropping changes |
| — | `:rename` | Rename (rewriting links) |
| — | `:trash` | Move to the trash |
| — | `:tab` | Tab N, new, close, next or prev |
| — | `:goto` | Go to a line |
| — | `:paste-image` | Paste an image as an attachment |
| — | `:attach` | Attach files |
| — | `:config` | Edit nib.toml |
| — | `:reindex` | Index the vault again |
| — | `:tag` | Notes by tag |
| — | `:edit` | Back to the source (:edit NOTE opens one) |
| — | `:outline` | Focus the outline |
| — | `:version` | Show nib's version |

<!-- registry:end -->

## Performance

Release builds, warm caches, deterministic 3,058-note / 40.2 MB fixture.
Workstation figures are development measurements; the Thinkpad is the target
machine. Backend measurements run over SSH on the Thinkpad; GUI measurements
are deferred by owner decision until GTK/GtkSourceView/cage are available there.
Workstation rows combine the Phase 10 backend trace with recorded UI measurements
from Phases 5–10; GUI figures are development measurements, not CI assertions.

| Metric | Thinkpad budget | Workstation measured | Thinkpad measured |
|---|---|---|---|
| Parse 5 KB note | 100 µs | 5.24 µs median / 10.13 µs p99 | 15.4 µs median / 22.6 µs p99 |
| Completion key path | 50 µs | 7.3 µs median / 17.6 µs p95 (app) | 0.84 µs (core); GUI pending |
| Index 3,058 / 30,058 notes | 400 ms / 4 s | 55.9 ms / 418 ms | 204 ms / 1.69 s |
| Search first rows visible | 100 ms | 4.0 ms median (GUI) | 1.26 ms first backend batch; GUI pending |
| Switcher visible / ranked rows | 30 ms / 10 ms | 8.4 ms / 5.9 ms median | 0.27 ms core matcher; GUI pending |
| Start to window mapped | 150 ms | 179 ms warm; misses 90 ms workstation target | GUI pending |
| Open 10,000-line note, first paint | 16 ms | 14.6 ms final run; misses 10 ms workstation target | GUI pending |
| Reading view, first part / whole note | 50 ms | 18.9 ms / 84.2 ms final run | 17.3 ms render ops only; GUI pending |
| Files column, 20,000 entries | 60 fps | 7.9 ms worst frame | 44.1 ms backend listing; GUI pending |
| Idle RSS, three tabs | 90 MB | 264.8 MB; misses budget (NVIDIA Vulkan) | GUI pending |

The workstation startup miss is mainly GTK Vulkan initialization (about 110 ms,
RTX 4090 plus llvmpipe ICDs); nib leaves renderer selection to GTK. Large reading
views render the first 64 KB promptly and finish in idle slices; the full-note
latency exceeds the original synchronous-render budget. The final three-tab run
used 264.8 MB RSS (158.3 MB PSS): 121.0 MB RSS came from graphics-library mappings;
one indexed tab already used 260.0 MB, and the next two added 4.7 MB. The final
10,000-line source first paint was 14.6 ms, above the 10 ms workstation target.
These misses are recorded rather than treated as passing budgets. See
[scope and measurements](.planning/SCOPE.md) for methods and exceptions.

## Development

```sh
scripts/mkfixture.sh --seed 7
scripts/dev-run.sh                   # isolated XDG dirs, confined fixture
scripts/check.sh                     # fmt, clippy, tests, docs, release, trace, smoke
scripts/agent/checks/run-all.sh NAME  # selected headless checks
scripts/agent/checks/run-all.sh --all # full UI suite, explicitly requested for release
NIB_ALLOW_ROOTS= cargo run --release -p nib-vault --example trace -- ~/.cache/nib-fixture
python3 scripts/generate-docs.py     # regenerate README keys and man-page source
```

Agents and development runs use fixtures. Phase 10 alone may copy the notes vault
into an isolated acceptance directory; the live vault is never edited or watched.
`scripts/agent/acceptance.sh COPY` verifies an isolated, committed copy below
`~/.cache/nib-acceptance/`, including a full before/after file manifest. Phase 10
found and fixed buffer-loss, rename-conflict and link-writing issues; see
[the review](CODE-REVIEW.md) and [owner handoff](HANDOFF-C.md).

Pure logic lives in nib-core; I/O and workers in nib-vault; GTK widgets in nib.
See [SCOPE](.planning/SCOPE.md), [Goal A](HANDOFF-A.md) and [Goal B](HANDOFF-B.md).

MIT licensed.
