# Goal C → owner

Phase 10 reviews and hardens the implementation, exercises a copied real vault,
and ships the private `v0.1.0` release. Findings and regression evidence are in
`CODE-REVIEW.md`; implementation and measurements are in `.planning/SCOPE.md`.

The repo remains `codedsleep/markdown-trekfm-rust`, on `master`. Commits are atomic;
no repository rename, live-vault migration or folio service change was performed.

## Acceptance and remaining checks

The isolated real-vault baseline is at
`~/.cache/nib-acceptance/phase10-20261016/vault`. The scripted session changes only
fourteen test linkers, their renamed target and a pasted image, all below
`nib-phase10-check/`. It verifies open-buffer undo and compares existing file
hashes, permissions and symlinks, including `.obsidian`. Earlier acceptance
artifacts were preserved.

Thinkpad backend traces run on generated fixtures under
`~/.cache/nib-phase10/`. GUI numbers are **pending by owner decision**, because
GTK, GtkSourceView and cage are absent there. Backend numbers do not stand in for
GUI startup, first paint, typing, finder latency, scrolling or RSS.

Owner desktop QA remains for dropping attachments, the attachment and first-run
folder choosers, and Hyprland activation-token behavior. Startup/RSS on the
workstation and complete large-note rendering miss their original targets;
measurements and causes are recorded, with GTK renderer selection left unchanged.

## Switch-over

Follow `.planning/DISCOVERY.md` §6 when ready:

1. Pull the current folio notes from zzzbox with its existing
   `deploy/pull-notes.sh`, first with `--dry-run`. The local notes folder is still
   the stale copy until this step.
2. Initialize and commit the live notes vault once, adding `.*.nib-tmp` to its
   `.gitignore` as described in discovery. Keep the acceptance copy separate.
3. Stop and disable the owner's `folio.service` on zzzbox; keep its volume as a
   backup for a month. These service changes belong to the owner.
4. Install the release on the workstation and Thinkpad with
   `install.sh -y --version v0.1.0` (private-repo `gh` authentication required),
   then add `bind = $mainMod, N, exec, nib` to Hyprland.
5. Use nib for a week with Obsidian and folio unopened, then inspect vault
   `git status`. That week and the pending desktop/Thinkpad GUI checks are the
   remaining owner acceptance, not a claim made by the scripted session.

Use `:w` to overwrite an external change explicitly, `:e!` to reload/discard, and
another tab to keep an unsaved buffer while opening a different note. Navigation
and closing retain unsaved edits; all 32 modified tabs block new-tab creation
until one is saved or closed.
