use bson::oid::ObjectId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Identificador polimórfico para modelos MongoDB, capaz de serializar y deserializar
/// transparentemente tanto `ObjectId` BSON como `String` nativo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelId {
    ObjectId(ObjectId),
    String(String),
}

impl ModelId {
    pub fn as_str(&self) -> String {
        match self {
            Self::ObjectId(oid) => oid.to_hex(),
            Self::String(s) => s.clone(),
        }
    }
}

impl From<ObjectId> for ModelId {
    fn from(oid: ObjectId) -> Self {
        Self::ObjectId(oid)
    }
}

impl From<String> for ModelId {
    fn from(s: String) -> Self {
        if let Ok(oid) = ObjectId::parse_str(&s) { Self::ObjectId(oid) } else { Self::String(s) }
    }
}

impl From<&str> for ModelId {
    fn from(s: &str) -> Self {
        s.to_string().into()
    }
}

impl Serialize for ModelId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::ObjectId(oid) => oid.serialize(serializer),
            Self::String(s) => s.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ModelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Oid(ObjectId),
            Str(String),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Oid(oid) => Ok(ModelId::ObjectId(oid)),
            Helper::Str(s) => Ok(s.into()),
        }
    }
}

/// Deserializa flexiblemente fechas que pueden venir en MongoDB como `Bson::DateTime`,
/// `chrono::DateTime<Utc>` o String en formato RFC3339.
pub fn deserialize_flexible_datetime<'de, D>(deserializer: D) -> Result<bson::DateTime, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Helper {
        Bson(bson::DateTime),
        Chrono(DateTime<Utc>),
        Str(String),
    }

    match Helper::deserialize(deserializer)? {
        Helper::Bson(dt) => Ok(dt),
        Helper::Chrono(dt) => Ok(bson::DateTime::from_chrono(dt)),
        Helper::Str(s) => {
            let dt = DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(serde::de::Error::custom)?;
            Ok(bson::DateTime::from_chrono(dt))
        }
    }
}

/// Deserializa opcionalmente fechas flexibles.
pub fn deserialize_optional_flexible_datetime<'de, D>(
    deserializer: D,
) -> Result<Option<bson::DateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Helper {
        Bson(bson::DateTime),
        Chrono(DateTime<Utc>),
        Str(String),
    }

    let opt: Option<Helper> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(Helper::Bson(dt)) => Ok(Some(dt)),
        Some(Helper::Chrono(dt)) => Ok(Some(bson::DateTime::from_chrono(dt))),
        Some(Helper::Str(s)) => {
            let dt = DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(serde::de::Error::custom)?;
            Ok(Some(bson::DateTime::from_chrono(dt)))
        }
    }
}
