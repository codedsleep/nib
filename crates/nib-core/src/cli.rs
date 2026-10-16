//! Argument parsing independent of `GApplication` and the filesystem.
use std::path::PathBuf;
/// Help displayed by the stub and eventual application.
pub const USAGE: &str = "Usage: nib [VAULT_DIR|NOTE.md] [--vault DIR] [--profile NAME]\n           [--new TITLE|--daily] [--version|--help]";
/// An invocation ready for the application.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Invocation {
    /// Explicit vault root.
    pub vault: Option<PathBuf>,
    /// Positional note or directory.
    pub path: Option<PathBuf>,
    /// Separate instance and state profile.
    pub profile: Option<String>,
    /// Requested note title.
    pub new: Option<String>,
    /// Open today's daily note.
    pub daily: bool,
    /// Print version.
    pub version: bool,
    /// Print usage.
    pub help: bool,
    /// Debug key injection, refused in release by the binary.
    pub test_keys: Option<String>,
    /// Debug state dump.
    pub test_dump: bool,
}
/// A malformed invocation.
#[derive(Debug, thiserror::Error)]
#[error("{0}\n{USAGE}")]
pub struct UsageError(pub String);
/// Parse arguments excluding `argv[0]`.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Invocation, UsageError> {
    let mut args = args.into_iter();
    let mut inv = Invocation::default();
    let mut positional = false;
    while let Some(a) = args.next() {
        if !positional && a == "--" {
            positional = true;
            continue;
        }
        if !positional && a.starts_with('-') {
            match a.as_str() {
                "--vault" | "--profile" | "--new" | "--test-keys" => {
                    let v = args
                        .next()
                        .filter(|s| !s.is_empty() && !s.starts_with("--"))
                        .ok_or_else(|| UsageError(format!("{a} requires a value")))?;
                    match a.as_str() {
                        "--vault" => {
                            if inv.vault.replace(v.into()).is_some() {
                                return Err(UsageError("duplicate --vault".into()));
                            }
                        }
                        "--profile" => {
                            if !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                                || inv.profile.replace(v).is_some()
                            {
                                return Err(UsageError(
                                    "profile must contain letters, digits or underscores".into(),
                                ));
                            }
                        }
                        "--new" => {
                            if inv.new.replace(v).is_some() {
                                return Err(UsageError("duplicate --new".into()));
                            }
                        }
                        _ => inv.test_keys = Some(v),
                    }
                }
                "--daily" => inv.daily = true,
                "--version" => inv.version = true,
                "--help" | "-h" => inv.help = true,
                "--test-dump" => inv.test_dump = true,
                _ => return Err(UsageError(format!("unknown option: {a}"))),
            }
        } else if inv.path.replace(a.into()).is_some() {
            return Err(UsageError("only one path may be opened".into()));
        }
    }
    if (inv.new.is_some() && inv.daily)
        || ((inv.new.is_some() || inv.daily) && inv.path.is_some())
        || (inv.version && inv.help)
    {
        return Err(UsageError("incompatible options".into()));
    }
    Ok(inv)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forms() {
        for s in [
            "",
            "vault",
            "note.md",
            "--new Title",
            "--daily",
            "--vault /tmp --daily",
            "--profile work --vault /tmp",
            "--version",
            "--help",
            "--test-keys j --test-dump",
        ] {
            assert!(
                parse(s.split_whitespace().map(str::to_owned)).is_ok(),
                "{s}"
            );
        }
        for s in [
            "--new",
            "--wat",
            "a b",
            "--daily --new hi",
            "--profile ../oops",
            "--daily n.md",
            "--vault a --vault b",
        ] {
            assert!(
                parse(s.split_whitespace().map(str::to_owned)).is_err(),
                "{s}"
            );
        }
    }
}
