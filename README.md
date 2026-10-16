# nib

A minimal, keyboard-first Markdown notes app for Linux, Rust, GTK 4 and
GtkSourceView 5. Opens an Obsidian vault unchanged. Development follows
[the scope](.planning/SCOPE.md); the GTK window arrives in Goal B.

Requires Rust 1.92+, GTK 4.14+ and GtkSourceView 5.10+.

```sh
scripts/check.sh                 # format, clippy, tests, boundaries, release stub
scripts/mkfixture.sh --seed 7     # isolated, deterministic test vault
scripts/dev-run.sh               # confined development entry point
cargo run -p nib -- --help
```

Before Phase 5 the binary supports `--help` and `--version`; other invocations
exit with status 2. Development and tests never use the live notes vault.
