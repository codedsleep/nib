//! Obsidian-compatible pasted-image names and collision suffixes.
use chrono::{DateTime, Local};
/// Base name for a pasted PNG.
pub fn pasted_image_name(now: DateTime<Local>) -> String {
    format!("Pasted image {}.png", now.format("%Y%m%d%H%M%S"))
}
/// Append an Obsidian-style numeric collision suffix before the extension.
pub fn collision_name(name: &str, n: usize) -> String {
    if n == 0 {
        return name.into();
    }
    name.rsplit_once('.').map_or_else(
        || format!("{name} {n}"),
        |(stem, ext)| format!("{stem} {n}.{ext}"),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn names() {
        assert_eq!(
            pasted_image_name(Local.with_ymd_and_hms(2026, 10, 16, 1, 2, 3).unwrap()),
            "Pasted image 20261016010203.png"
        );
        assert_eq!(collision_name("a.png", 2), "a 2.png");
    }
}
