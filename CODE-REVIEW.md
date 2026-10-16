# Phase 10 code review

Reviewed against `.planning/SCOPE.md`, `.planning/DISCOVERY.md`, `.planning/PROMPT.md`
and both handoffs on 2026-10-16. The repository's initial commit is `1976a54`.
The review mapped the changes across core, vault, application, scripts and release
data to the acceptance lines, concentrating on file integrity, user-controlled
paths, navigation, asynchronous operations and packaging. This is a local,
single-user application; no authentication or server boundary exists.

Nine confirmed findings were fixed. No unresolved high-severity finding remains
in the reviewed paths. The platform and owner validation limits below remain.

## Findings and fixes

1. **High — navigation and closing could lose unsaved buffers.**
   `crates/nib/src/app.rs` (`load_into`, window close handler) ignored a failed
   save and replaced or destroyed the buffer. Autosave-disabled navigation also
   saved unexpectedly; configuration tabs could be saved on navigation.
   Replacement now requires a successful automatic save or a clean buffer;
   closing asks before discarding any remaining edits. History advances only
   after a successful load. Proof: `unsaved.sh`, `external.sh`, `autosave.sh`.
2. **High — watcher delay left an autosave overwrite window.**
   `crates/nib-vault/src/service.rs` (`save_checked`) previously checked UTF-8
   validity but not whether disk contents still matched the loaded snapshot.
   `save_unchanged` now checks the decoded text fingerprint, BOM and line-ending
   format before automatic writes, and refuses a missing file. Explicit `:w`
   retains overwrite/recreate semantics. Proof: the integration test
   `autosave_checks_the_disk_before_watcher_delivery`, with watching disabled.
   This is a pre-write check, not a filesystem transaction against other writers;
   an uncooperative process can still race between the check and atomic rename.
3. **High — rename conflict results could rewrite links incorrectly.**
   `crates/nib/src/fileops.rs` (`done`) used the requested name even after Skip or
   Keep both. The backend now returns the actual rename destination. Skip applies
   no edits; Keep both replans the approved rewrites against the pre-move index.
   Renaming also stops if modified note buffers cannot be saved safely.
   Proof: `renameconflict.sh` checks all fourteen sources and the open tab.
4. **High — reading task clicks bypassed conflict protection.**
   `crates/nib/src/reading.rs` (`toggle_task`) called the explicit overwrite save
   path. It now uses the automatic-save policy, retaining the changed buffer
   without overwriting an external edit. Proof: `readtask.sh`.
5. **Medium — the tab limit could evict the tab being opened.**
   `crates/nib/src/tabs.rs` assigned the new tab a zero LRU timestamp and considered
   it for eviction before selecting/loading it. The newly added tab is now
   excluded. When all 32 buffers have edits, creation is refused with a notice;
   opening configuration cannot repurpose an existing tab. Proof: `tablimit.sh`.
6. **Medium — generated links could resolve to another file.**
   `crates/nib-core/src/links.rs` emitted descendant relative wikilinks without
   the `./` needed to distinguish them from vault-root paths. Attachment insertion
   also dropped folders even when basenames collided. Shared writing helpers now
   preserve unambiguous paths and apply the configured format to inserted and
   completed note/attachment links. Proof: the links integration test
   `emitted_relative_links_and_duplicate_attachments_resolve_to_their_target`,
   plus `complete.sh` and `paste.sh`.
7. **Medium — a clean release runner could not run integration tests.**
   `.github/workflows/release.yml` omitted the fixture required by the vault
   integration test. Its floating stable compiler also selected Rust 1.99,
   introducing new clippy failures after the local 1.98.1 validation. The
   unpublished tag was replaced with a commit pinning CI to Rust 1.98.1. The
   workflow now generates the fixture before testing, checks
   generated documentation/API docs, and exercises packaged installation.
   Proof: local `scripts/check.sh` and the green [tag workflow](https://github.com/codedsleep/markdown-trekfm-rust/actions/runs/38857984565). The published Ubuntu archive also installs and maps a fixture window on the workstation.
8. **Low — documented logging levels were ignored.**
   `crates/nib-vault/src/log.rs` treated `error` and `off` as `warn`. Both now map
   to their documented levels. Proof: `log::tests::documented_levels`.
9. **Low — sessions saved cursor columns but did not restore them.**
   `crates/nib/src/session.rs` now restores the saved character column, clamped to
   the line end. The state documentation now agrees with GTK's character offset.
   Proof: the two-session `session.sh` check asserts a nonzero column.

## Safety and scope evidence

- All application mutations cross `nib-vault`'s guarded filesystem boundary.
  `remove_file` occurs only for owned temporary files; no `remove_dir_all` or
  permanent-delete operation exists in application/vault code. Replacement uses
  XDG trash. Integration tests exercise guard refusals for every mutation.
- The kill test interrupts 200 writers and checks complete old/new bytes plus
  stale-temp recovery. Symlink saves, permissions, non-UTF-8 refusals and exact
  BOM/CRLF/mixed-ending round trips have backend and headless coverage.
- Launch surfaces converge on the URL and executable classifier. `follow.sh`
  verifies executable/desktop and `file:`/`javascript:` refusals without launches.
- A strace of the complete reading check, including twenty local images and a
  remote image, recorded zero `socket(AF_INET...)`/`socket(AF_INET6...)` calls.
  Local Unix IPC and GLib's local route-monitor netlink socket remain expected.
  The application contains no remote-image fetch path or template execution.
- Fresh acceptance copy:
  `~/.cache/nib-acceptance/phase10-20261016/vault`. `scripts/agent/acceptance.sh`
  checks fourteen rewrites, open-buffer undo, edits, clipboard-image saving and
  rendering, then compares every existing file's content hash, permissions and
  symlink target, including ignored `.obsidian` files. Only sixteen explicitly
  created notes/attachments differ from the baseline. The live vault was copied
  once and was never opened or watched by nib.
- Final local gate: `scripts/check.sh` passes for 0.1.0 (68 tests, four opt-in/helper tests ignored); core line coverage is 94.85%; the full UI suite passes all 27 checks / 312 assertions.
- The generated man page renders with scdoc/man; desktop-file validation and
  temporary-prefix install/uninstall/checksum rejection checks pass.

## Residual limits

- Thinkpad GUI performance is pending by owner decision. Backend traces run over
  SSH on generated fixtures; they do not establish UI latency or RSS.
- Workstation warm startup (178.9 ms), RSS (264.8 MB) and the final source first paint (14.6 ms versus the workstation 10 ms target) exceed their targets with GTK's NVIDIA Vulkan
  renderer; the previously measured large-note complete reading render also
  exceeds the synchronous budget. First paint and complete rendering are recorded
  separately. nib leaves renderer selection to GTK.
- Native drag-and-drop, portal attachment/first-run choosers and Hyprland
  activation-token delivery still need owner desktop QA. The headless tests cover
  the underlying clipboard, launch, file operation and single-instance paths.
- The real-vault check is a scripted isolated-copy session. The required week of
  daily use and the folio/Obsidian switch-over belong to the owner.
- The filesystem policy assumes a single local user. It is not a security
  sandbox against a concurrent adversary changing symlinks between checks and I/O.
