//! Tolerant TOML configuration with field-local defaults and warnings.
use serde::{Deserialize, Serialize};
/// Commented first-run configuration.
pub const DEFAULT_TOML: &str = r#"# nib configuration. Edit with :config; fonts and editor settings apply live.
# Example vault.last = "/home/zzz/Documents/notes"; no vault is opened implicitly here.
# Vault folders apply on the next vault open; every path stays below the vault.
[vault]
last = ""
new_folder = ""
attachments = "attachments"
daily_folder = "daily"
daily_format = "%Y-%m-%d"
daily_template = ""
templates = "templates"
[editor]
vim = true
autosave = true
font = ""
line_numbers = false
wrap = true
max_width = 0
margin = 24
auto_pair = ["[[", "`"]
style_scheme = "nib-nord"
reading_font = ""
[links]
format = "shortest"
rewrite_on_rename = "ask"
[files]
only_markdown = false
show_hidden = false
sort = "name"
show_parent_column = false
linemode = "none"
"#;
/// A recoverable configuration issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning(pub String);
/// Preferred emitted link notation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkFormat {
    /// Unique stem, else path.
    #[default]
    Shortest,
    /// Relative to source folder.
    Relative,
    /// Vault-root relative.
    Absolute,
}
/// Link rewrite policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rewrite {
    /// Ask first.
    #[default]
    Ask,
    /// Rewrite automatically.
    Always,
    /// Leave links alone.
    Never,
}
/// Directory sort order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// Natural filename order.
    #[default]
    Name,
    /// Most recently modified first.
    Modified,
}
/// Secondary row label.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Linemode {
    /// No secondary label.
    #[default]
    None,
    /// Modification age.
    Modified,
}
/// Disk-layout preferences, all relative except last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    /// Last opened vault.
    pub last: String,
    /// New-note directory, or current.
    pub new_folder: String,
    /// Attachment directory.
    pub attachments: String,
    /// Daily note directory.
    pub daily_folder: String,
    /// Daily filename strftime pattern.
    pub daily_format: String,
    /// Optional template name.
    pub daily_template: String,
    /// Template directory.
    pub templates: String,
}
impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            last: String::new(),
            new_folder: String::new(),
            attachments: "attachments".into(),
            daily_folder: "daily".into(),
            daily_format: "%Y-%m-%d".into(),
            daily_template: String::new(),
            templates: "templates".into(),
        }
    }
}
/// Editor and reading-view preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    /// Enable `GtkSourceView` vim.
    pub vim: bool,
    /// Save after idle and navigation.
    pub autosave: bool,
    /// Empty uses the desktop monospace font.
    pub font: String,
    /// Show gutter numbers.
    pub line_numbers: bool,
    /// Word wrapping.
    pub wrap: bool,
    /// Centered width, zero for unrestricted.
    pub max_width: u32,
    /// Horizontal margin in pixels.
    pub margin: u32,
    /// Enabled opening delimiters.
    pub auto_pair: Vec<String>,
    /// `GtkSourceView` scheme identifier.
    pub style_scheme: String,
    /// Empty uses the desktop interface font.
    pub reading_font: String,
}
impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            vim: true,
            autosave: true,
            font: String::new(),
            line_numbers: false,
            wrap: true,
            max_width: 0,
            margin: 24,
            auto_pair: vec!["[[".into(), "`".into()],
            style_scheme: "nib-nord".into(),
            reading_font: String::new(),
        }
    }
}
/// Link behavior preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LinksConfig {
    /// New link notation.
    pub format: LinkFormat,
    /// Rename rewriting.
    pub rewrite_on_rename: Rewrite,
}
/// Files-column preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesConfig {
    /// Filter non-notes.
    pub only_markdown: bool,
    /// Show ordinary dotfiles.
    pub show_hidden: bool,
    /// Initial order.
    pub sort: Sort,
    /// Optional parent list.
    pub show_parent_column: bool,
    /// Row detail.
    pub linemode: Linemode,
}
/// Complete settings tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Vault paths.
    pub vault: VaultConfig,
    /// Editor behavior.
    pub editor: EditorConfig,
    /// Link preferences.
    pub links: LinksConfig,
    /// Files column.
    pub files: FilesConfig,
}
impl Config {
    /// Merge valid fields individually; invalid and unknown fields warn.
    pub fn parse(input: &str) -> (Self, Vec<Warning>) {
        let mut warnings = Vec::new();
        let mut base =
            toml::Value::try_from(Self::default()).expect("Config serializes as a TOML table");
        match toml::from_str::<toml::Value>(input) {
            Ok(v) => merge(&mut base, &v, "", &mut warnings),
            Err(e) => warnings.push(Warning(e.to_string())),
        }
        (
            base.try_into()
                .expect("only validated Config fields are merged"),
            warnings,
        )
    }
}
fn merge(
    base: &mut toml::Value,
    incoming: &toml::Value,
    prefix: &str,
    warnings: &mut Vec<Warning>,
) {
    let (Some(dst), Some(src)) = (base.as_table_mut(), incoming.as_table()) else {
        warnings.push(Warning(format!("{prefix}: expected table")));
        return;
    };
    for (k, v) in src {
        let path = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        let Some(old) = dst.get_mut(k) else {
            warnings.push(Warning(format!("unknown key: {path}")));
            continue;
        };
        if old.is_table() {
            merge(old, v, &path, warnings);
            continue;
        }
        let valid = std::mem::discriminant(old) == std::mem::discriminant(v)
            && match path.as_str() {
                "links.format" => v
                    .as_str()
                    .is_some_and(|s| ["shortest", "relative", "absolute"].contains(&s)),
                "links.rewrite_on_rename" => v
                    .as_str()
                    .is_some_and(|s| ["ask", "always", "never"].contains(&s)),
                "files.sort" => v
                    .as_str()
                    .is_some_and(|s| ["name", "modified"].contains(&s)),
                "files.linemode" => v
                    .as_str()
                    .is_some_and(|s| ["none", "modified"].contains(&s)),
                "editor.margin" | "editor.max_width" => {
                    v.as_integer().is_some_and(|n| u32::try_from(n).is_ok())
                }
                "editor.auto_pair" => v
                    .as_array()
                    .is_some_and(|a| a.iter().all(toml::Value::is_str)),
                _ => true,
            };
        if valid {
            *old = v.clone();
        } else {
            warnings.push(Warning(format!("invalid value: {path}; using default")));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_bad_fields() {
        assert_eq!(Config::parse(DEFAULT_TOML), (Config::default(), vec![]));
        let (c, w) = Config::parse(
            "[editor]\nvim=false\nmargin=-1\nwrap='yes'\nwat=2\n[links]\nformat='bogus'",
        );
        assert!(!c.editor.vim);
        assert_eq!(c.editor.margin, 24);
        assert_eq!(w.len(), 4);
        assert!(!Config::parse("oops = [").1.is_empty());
        assert_eq!(Config::parse("").0, Config::default());
    }
}
