//! Inert substitution of date, time and title placeholders.
use chrono::{DateTime, Local};
/// All template values, supplied by the caller for deterministic tests.
#[derive(Debug, Clone)]
pub struct TemplateVars {
    /// Local instant.
    pub now: DateTime<Local>,
    /// Note title.
    pub title: String,
    /// Default date pattern.
    pub date_format: String,
}
/// Expand only known placeholders; unknown and malformed ones remain verbatim.
pub fn expand(text: &str, vars: &TemplateVars) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find("}}") else {
            break;
        };
        let token = &rest[2..end];
        let value = match token {
            "date" => format_date(vars, &vars.date_format),
            "time" => Some(vars.now.format("%H:%M").to_string()),
            "title" => Some(vars.title.clone()),
            _ => token
                .strip_prefix("date:")
                .and_then(|f| format_date(vars, f)),
        };
        out.push_str(value.as_deref().unwrap_or(&rest[..end + 2]));
        rest = &rest[end + 2..];
    }
    out.push_str(rest);
    out
}
fn format_date(vars: &TemplateVars, fmt: &str) -> Option<String> {
    use std::fmt::Write;
    let mut s = String::new();
    write!(s, "{}", vars.now.format(fmt)).ok()?;
    Some(s)
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn substitution() {
        let v = TemplateVars {
            now: Local.with_ymd_and_hms(2026, 10, 16, 12, 34, 0).unwrap(),
            title: "Hello".into(),
            date_format: "%Y-%m-%d".into(),
        };
        assert_eq!(
            expand(
                "{{title}} {{date}} {{date:%Y}} {{time}} {{unknown}} $(rm -rf ~) {{",
                &v
            ),
            "Hello 2026-10-16 2026 12:34 {{unknown}} $(rm -rf ~) {{"
        );
        assert_eq!(expand("{{date:%Q}}", &v), "{{date:%Q}}");
    }
}
