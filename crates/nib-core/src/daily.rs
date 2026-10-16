//! Calendar-to-note identities without filesystem access.
use crate::{IdError, NoteId, settings::VaultConfig};
use chrono::{Duration, NaiveDate};
/// Compute a daily note path from the configured directory and strftime pattern.
pub fn path(cfg: &VaultConfig, date: NaiveDate) -> Result<NoteId, IdError> {
    use std::fmt::Write;
    let mut name = String::new();
    write!(name, "{}", date.format(&cfg.daily_format))
        .map_err(|_| IdError(cfg.daily_format.clone()))?;
    NoteId::new(format!("{}/{}", cfg.daily_folder, name).trim_start_matches('/'))
}
/// Resolve a day offset from a date.
pub fn offset(cfg: &VaultConfig, from: NaiveDate, days: i64) -> Result<NoteId, IdError> {
    let date = from
        .checked_add_signed(
            Duration::try_days(days).ok_or_else(|| IdError("daily offset overflow".into()))?,
        )
        .ok_or_else(|| IdError("daily date overflow".into()))?;
    path(cfg, date)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates() {
        let c = VaultConfig::default();
        let d = NaiveDate::from_ymd_opt(2024, 2, 28).unwrap();
        assert_eq!(offset(&c, d, 1).unwrap().as_str(), "daily/2024-02-29");
        assert_eq!(offset(&c, d, 2).unwrap().as_str(), "daily/2024-03-01");
    }
}
