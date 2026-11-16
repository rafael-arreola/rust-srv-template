use std::{fmt, ops::Deref};

use chrono::{DateTime, SecondsFormat, Utc};
#[cfg(feature = "mongo")]
use mongodb::bson;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[cfg(feature = "mongo")]
use crate::cast::is_db_mode;

/// Instante UTC compatible con `chrono::DateTime<Utc>`, HTTP ISO-8601 y (con feature `mongo`) MongoDB `ISODate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcDateTime(pub DateTime<Utc>);

impl UtcDateTime {
    #[inline]
    pub fn now() -> Self {
        let ms = Utc::now().timestamp_millis();
        Self(DateTime::from_timestamp_millis(ms).unwrap_or_else(Utc::now))
    }

    #[inline]
    pub const fn from_chrono(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }

    #[inline]
    pub fn from_timestamp_secs(secs: i64) -> Option<Self> {
        DateTime::from_timestamp(secs, 0).map(Self)
    }

    #[inline]
    pub const fn into_inner(self) -> DateTime<Utc> {
        self.0
    }

    #[cfg(feature = "mongo")]
    #[inline]
    pub(crate) fn to_bson(self) -> bson::DateTime {
        bson::DateTime::from_chrono(self.0)
    }
}

impl Default for UtcDateTime {
    fn default() -> Self {
        Self::now()
    }
}

impl Deref for UtcDateTime {
    type Target = DateTime<Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<DateTime<Utc>> for UtcDateTime {
    fn from(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }
}

impl From<UtcDateTime> for DateTime<Utc> {
    fn from(dt: UtcDateTime) -> Self {
        dt.0
    }
}

#[cfg(feature = "mongo")]
impl From<UtcDateTime> for bson::DateTime {
    fn from(dt: UtcDateTime) -> Self {
        bson::DateTime::from_chrono(dt.0)
    }
}

impl fmt::Display for UtcDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}

impl Serialize for UtcDateTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[cfg(feature = "mongo")]
        if is_db_mode() {
            return bson::DateTime::from_chrono(self.0).serialize(serializer);
        }

        serializer.serialize_str(&self.0.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}

impl<'de> Deserialize<'de> for UtcDateTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[cfg(feature = "mongo")]
        if is_db_mode() {
            return match bson::Bson::deserialize(deserializer)? {
                bson::Bson::DateTime(bdt) => Ok(Self(bdt.to_chrono())),
                bson::Bson::String(s) => DateTime::parse_from_rfc3339(s.trim())
                    .map(|dt| Self(dt.with_timezone(&Utc)))
                    .map_err(de::Error::custom),
                other => Err(de::Error::custom(format!(
                    "expected BSON DateTime or RFC 3339 String, got {:?}",
                    other.element_type()
                ))),
            };
        }

        struct IsoVisitor;

        impl de::Visitor<'_> for IsoVisitor {
            type Value = UtcDateTime;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an RFC 3339 date-time string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                DateTime::parse_from_rfc3339(v.trim())
                    .map(|dt| UtcDateTime(dt.with_timezone(&Utc)))
                    .map_err(E::custom)
            }
        }

        deserializer.deserialize_str(IsoVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_utc_z_and_converts_client_offsets_to_utc() {
        let dt = UtcDateTime::from_chrono(
            DateTime::from_timestamp_millis(1_700_000_000_123).expect("valid ms"),
        );
        let json = serde_json::to_string(&dt).expect("json encode");
        assert_eq!(json, "\"2023-11-14T22:13:20.123Z\"");

        let parsed: UtcDateTime =
            serde_json::from_str("\"2023-11-14T16:13:20.123-06:00\"").expect("offset decode");
        assert_eq!(parsed, dt);
    }
}
