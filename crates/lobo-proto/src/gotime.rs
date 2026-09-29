use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

const ZERO_TEXT: &str = "0001-01-01T00:00:00Z";

/// Go's zero time and RFC3339Nano formatting, including the original UTC offset.
#[derive(Debug, Clone, PartialEq, Eq, Default, TS)]
#[ts(export, type = "string")]
pub struct GoTime(pub Option<DateTime<FixedOffset>>);

impl GoTime {
    pub const ZERO: Self = Self(None);

    pub fn is_zero(&self) -> bool {
        self.0.is_none()
    }

    pub fn from_utc(t: DateTime<Utc>) -> Self {
        Self(Some(t.fixed_offset()))
    }
}

impl Serialize for GoTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Some(t) = self.0 else {
            return serializer.serialize_str(ZERO_TEXT);
        };
        if !(0..=9999).contains(&t.year()) || t.nanosecond() >= 1_000_000_000 {
            return Err(serde::ser::Error::custom(
                "time is outside Go RFC3339 range",
            ));
        }
        let mut out = t.format("%Y-%m-%dT%H:%M:%S").to_string();
        if t.nanosecond() != 0 {
            let fraction = format!("{:09}", t.nanosecond());
            out.push('.');
            out.push_str(fraction.trim_end_matches('0'));
        }
        if t.offset().local_minus_utc() == 0 {
            out.push('Z');
        } else {
            out.push_str(&t.format("%:z").to_string());
        }
        serializer.serialize_str(&out)
    }
}

impl<'de> Deserialize<'de> for GoTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        if s == ZERO_TEXT {
            return Ok(Self::ZERO);
        }
        DateTime::parse_from_rfc3339(&s)
            .map(|t| Self(Some(t)))
            .map_err(serde::de::Error::custom)
    }
}
