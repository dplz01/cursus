//! How Cursus writes times of day: the 12-hour or 24-hour clock, chosen in
//! Settings → Appearance and used for message stamps, the chat list and
//! message info alike. Shared by every Cursus app so they write the same.

use chrono::{DateTime, TimeZone};
use serde::{Deserialize, Serialize};

/// The user's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TimeFormat {
    /// `2:05 PM` (the default, as before the setting existed).
    #[default]
    #[serde(rename = "12h")]
    H12,
    /// `14:05`.
    #[serde(rename = "24h")]
    H24,
}

impl TimeFormat {
    pub const ALL: [Self; 2] = [Self::H12, Self::H24];

    pub fn label(self) -> &'static str {
        match self {
            Self::H12 => "12-hour",
            Self::H24 => "24-hour",
        }
    }

    /// Wire / storage value: `12h` or `24h` (anything else is 12-hour).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::H12 => "12h",
            Self::H24 => "24h",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        if s.trim() == "24h" {
            Self::H24
        } else {
            Self::H12
        }
    }

    /// The chrono pattern for a time of day: `%-I:%M %p` / `%H:%M`.
    pub fn pattern(self) -> &'static str {
        match self {
            Self::H12 => "%-I:%M %p",
            Self::H24 => "%H:%M",
        }
    }

    /// A time of day: `2:05 PM` or `14:05`.
    pub fn time<Tz: TimeZone>(self, at: &DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        at.format(self.pattern()).to_string()
    }

    /// Date and time, as in message info: `3 Nov 2026, 2:05 PM` / `3 Nov 2026, 14:05`.
    pub fn date_time<Tz: TimeZone>(self, at: &DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        format!("{}, {}", at.format("%-d %b %Y"), self.time(at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn both_clocks() {
        let at = Utc.with_ymd_and_hms(2026, 11, 3, 14, 5, 0).unwrap();
        assert_eq!(TimeFormat::H12.time(&at), "2:05 PM");
        assert_eq!(TimeFormat::H24.time(&at), "14:05");
        let morning = Utc.with_ymd_and_hms(2026, 11, 3, 9, 7, 0).unwrap();
        assert_eq!(TimeFormat::H12.time(&morning), "9:07 AM");
        assert_eq!(TimeFormat::H24.time(&morning), "09:07");
        assert_eq!(TimeFormat::H24.date_time(&at), "3 Nov 2026, 14:05");
        assert_eq!(TimeFormat::H12.date_time(&at), "3 Nov 2026, 2:05 PM");
    }

    #[test]
    fn stored_values_round_trip() {
        for f in TimeFormat::ALL {
            assert_eq!(TimeFormat::from_str_lossy(f.as_str()), f);
            assert_eq!(serde_json::to_string(&f).unwrap(), format!("\"{}\"", f.as_str()));
        }
        assert_eq!(TimeFormat::from_str_lossy("nonsense"), TimeFormat::H12);
    }
}
