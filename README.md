# nib

A minimal, keyboard-first Markdown notes app for Linux, Rust, GTK 4 and
GtkSourceView 5. Opens an Obsidian vault unchanged. Development follows
[the scope](.planning/SCOPE.md); the GTK window arrives in Goal B.

Goal A is complete through Phase 4. See [HANDOFF-A.md](HANDOFF-A.md) for the
backend API, event wiring, measured budgets and remaining frontend work.

Requires Rust 1.92+, GTK 4.14+ and GtkSourceView 5.10+.

```sh
scripts/check.sh                 # format, clippy, tests, boundaries, release stub
scripts/mkfixture.sh --seed 7     # isolated, deterministic test vault
scripts/dev-run.sh               # confined development entry point
cargo run -p nib -- --help
```

Goal A provides the pure Markdown/editor algorithms and vault services: guarded
atomic saves, cached listing, parallel indexing, watched changes, streamed search
and matching, file operations and theme discovery. To measure a generated fixture:

```sh
NIB_ALLOW_ROOTS= cargo run --release -p nib-vault --example trace -- ~/.cache/nib-fixture
```

Before Phase 5 the binary supports `--help` and `--version`; other invocations
exit with status 2. Development and tests never use the live notes vault.
