# Changelog

All notable changes to nib are documented here, following Keep a Changelog.

## [0.1.0] - 2026-10-16

### Added

- GTK 4 Markdown editor with GtkSourceView vim modes and a keyboard-driven files column.
- Wikilinks, Markdown links, backlinks, tags, outline and fuzzy completion.
- Quick switcher, command palette and streamed full-text vault search.
- Source, reading and split views with local images, note embeds, callouts and tasks.
- Daily notes, templates, pasted screenshots, file attachments, tabs and sessions.
- Atomic, format-preserving saves, external-change protection and XDG trash.
- Executable attachment refusal, URL scheme restrictions and development confinement.
- Authenticated release installer, local installer, desktop integration and manual.

### Known limitations

- GtkSourceView vim does not support :map or gf; substitution is unreliable in 5.20.
- GPU initialization exceeds the workstation startup target; Thinkpad validation is pending runtime setup.
- Large reading views show their first portion immediately and complete in idle slices.
- Release archives support Linux x86_64; ARM users build from source.
