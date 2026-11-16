use std::{
    cell::Cell,
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::Deref,
    str::FromStr,
};

#[cfg(feature = "mongo")]
pub use mongodb::bson::oid::ObjectId;
#[cfg(feature = "mongo")]
use mongodb::bson::{self, Bson};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
#[cfg(feature = "uuid")]
pub use uuid::Uuid;

use crate::error::AppError;

thread_local! {
    static DB_MODE: Cell<bool> = const { Cell::new(false) };
}

/// Activa el modo de serialización/deserialización nativa de base de datos durante `f`.
///
/// Fuera de este alcance, `Cast<T, Db>` serializa/deserializa usando `T` (vista cliente/JSON);
/// dentro de este alcance, usa `Db` (representación de almacenamiento).
pub fn with_db_mode<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    DB_MODE.with(|mode| {
        struct Guard(bool);
        impl Drop for Guard {
            fn drop(&mut self) {
                DB_MODE.with(|m| m.set(self.0));
            }
        }
        let _guard = Guard(mode.replace(true));
        f()
    })
}

#[inline]
pub fn is_db_mode() -> bool {
    DB_MODE.with(Cell::get)
}

/// Contrato de puente entre el tipo visto en Rust/JSON (`T`) y su representación en BD (`Db`).
pub trait DbRepr<T>: Sized {
    fn validate(app: &T) -> Result<(), String>;
    fn to_db(app: &T) -> Result<Self, String>;
    fn from_db(db: Self) -> Result<T, String>;
}

/// Identidad directa (`Cast<i64>`, `Cast<i32>`, `Cast<u64>`, `Cast<String>`): mismo tipo en JSON y en BD.
impl<T: Clone> DbRepr<T> for T {
    #[inline]
    fn validate(_app: &T) -> Result<(), String> {
        Ok(())
    }

    #[inline]
    fn to_db(app: &T) -> Result<Self, String> {
        Ok(app.clone())
    }

    #[inline]
    fn from_db(db: Self) -> Result<T, String> {
        Ok(db)
    }
}

/// Puente `Cast<String, i64>`: String numérico en JSON <-> `BIGINT` (SQL Server, Postgres, etc.).
impl DbRepr<String> for i64 {
    #[inline]
    fn validate(app: &String) -> Result<(), String> {
        app.trim()
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| "expected a valid 64-bit integer string".to_string())
    }

    #[inline]
    fn to_db(app: &String) -> Result<Self, String> {
        app.trim().parse::<i64>().map_err(|_| "expected a valid 64-bit integer string".to_string())
    }

    #[inline]
    fn from_db(db: Self) -> Result<String, String> {
        Ok(db.to_string())
    }
}

/// Puente `Cast<String, i32>`: String numérico en JSON <-> `INT` en SQL.
impl DbRepr<String> for i32 {
    #[inline]
    fn validate(app: &String) -> Result<(), String> {
        app.trim()
            .parse::<i32>()
            .map(|_| ())
            .map_err(|_| "expected a valid 32-bit integer string".to_string())
    }

    #[inline]
    fn to_db(app: &String) -> Result<Self, String> {
        app.trim().parse::<i32>().map_err(|_| "expected a valid 32-bit integer string".to_string())
    }

    #[inline]
    fn from_db(db: Self) -> Result<String, String> {
        Ok(db.to_string())
    }
}

/// Puente `Cast<String, u64>`: String numérico sin signo en JSON <-> `u64` en BD.
impl DbRepr<String> for u64 {
    #[inline]
    fn validate(app: &String) -> Result<(), String> {
        app.trim()
            .parse::<u64>()
            .map(|_| ())
            .map_err(|_| "expected a valid unsigned 64-bit integer string".to_string())
    }

    #[inline]
    fn to_db(app: &String) -> Result<Self, String> {
        app.trim()
            .parse::<u64>()
            .map_err(|_| "expected a valid unsigned 64-bit integer string".to_string())
    }

    #[inline]
    fn from_db(db: Self) -> Result<String, String> {
        Ok(db.to_string())
    }
}

/// Puente `Cast<String, Uuid>` (requiere feature `uuid`):
/// String canónico con guiones en Rust/JSON <-> `UUID` / `UNIQUEIDENTIFIER` nativo en SQL Server o Postgres.
#[cfg(feature = "uuid")]
impl DbRepr<String> for Uuid {
    #[inline]
    fn validate(app: &String) -> Result<(), String> {
        Uuid::parse_str(app.trim())
            .map(|_| ())
            .map_err(|_| "expected a valid UUID string".to_string())
    }

    #[inline]
    fn to_db(app: &String) -> Result<Self, String> {
        Uuid::parse_str(app.trim()).map_err(|_| "expected a valid UUID string".to_string())
    }

    #[inline]
    fn from_db(db: Self) -> Result<String, String> {
        Ok(db.hyphenated().to_string())
    }
}

/// Identificador flexible para colecciones MongoDB heredadas que mezclan `_id: ObjectId` y `_id: String`.
///
/// Úsalo como `Cast<String, FlexibleId>`: hacia HTTP siempre es `String`; hacia MongoDB convierte a
/// `ObjectId` si tiene 24 caracteres hexadecimales válidos o preserva `String` en caso contrario.
#[cfg(feature = "mongo")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlexibleId {
    ObjectId(ObjectId),
    String(String),
}

#[cfg(feature = "mongo")]
impl Serialize for FlexibleId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::ObjectId(oid) => oid.serialize(serializer),
            Self::String(s) => s.serialize(serializer),
        }
    }
}

#[cfg(feature = "mongo")]
impl<'de> Deserialize<'de> for FlexibleId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Bson::deserialize(deserializer)? {
            Bson::ObjectId(oid) => Ok(Self::ObjectId(oid)),
            Bson::String(s) => Ok(match ObjectId::parse_str(s.trim()) {
                Ok(oid) => Self::ObjectId(oid),
                Err(_) => Self::String(s),
            }),
            other => Err(de::Error::custom(format!(
                "expected BSON ObjectId or String for FlexibleId, got {:?}",
                other.element_type()
            ))),
        }
    }
}

#[cfg(feature = "mongo")]
impl DbRepr<String> for FlexibleId {
    #[inline]
    fn validate(app: &String) -> Result<(), String> {
        if app.trim().is_empty() {
            return Err("identifier cannot be empty".to_string());
        }
        Ok(())
    }

    #[inline]
    fn to_db(app: &String) -> Result<Self, String> {
        let trimmed = app.trim();
        if trimmed.is_empty() {
            return Err("identifier cannot be empty".to_string());
        }
        Ok(match ObjectId::parse_str(trimmed) {
            Ok(oid) => Self::ObjectId(oid),
            Err(_) => Self::String(trimmed.to_string()),
        })
    }

    #[inline]
    fn from_db(db: Self) -> Result<String, String> {
        Ok(match db {
            Self::ObjectId(oid) => oid.to_hex(),
            Self::String(s) => s,
        })
    }
}

/// Puente `Cast<String, ObjectId>` (requiere feature `mongo`):
/// String hex de 24 caracteres en Rust/JSON <-> `BSON ObjectId` nativo en MongoDB.
#[cfg(feature = "mongo")]
impl DbRepr<String> for ObjectId {
    #[inline]
    fn validate(app: &String) -> Result<(), String> {
        ObjectId::parse_str(app.trim())
            .map(|_| ())
            .map_err(|_| "expected a valid 24-character hex identifier".to_string())
    }

    #[inline]
    fn to_db(app: &String) -> Result<Self, String> {
        ObjectId::parse_str(app.trim())
            .map_err(|_| "expected a valid 24-character hex identifier".to_string())
    }

    #[inline]
    fn from_db(db: Self) -> Result<String, String> {
        Ok(db.to_hex())
    }
}

/// Traductor declarativo de tipo estilo `$casts` de Laravel: `Cast<T, Db = T>`.
///
/// - En memoria y hacia clientes HTTP opera como `T`.
/// - Hacia la base de datos valida y traduce hacia/desde `Db` (`ObjectId`, `Uuid`, `i64`, `String`...).
pub struct Cast<T, Db = T> {
    value: T,
    _db: PhantomData<fn() -> Db>,
}

#[cfg(feature = "mongo")]
impl Cast<String, ObjectId> {
    #[inline]
    pub fn generate() -> Self {
        Self { value: ObjectId::new().to_hex(), _db: PhantomData }
    }

    #[inline]
    pub fn from_oid(oid: ObjectId) -> Self {
        Self { value: oid.to_hex(), _db: PhantomData }
    }
}

#[cfg(feature = "mongo")]
impl Default for Cast<String, ObjectId> {
    fn default() -> Self {
        Self::generate()
    }
}

#[cfg(feature = "uuid")]
impl Cast<String, Uuid> {
    /// Genera un `UUID v7` ordenable por tiempo (óptimo para índices B-tree / clustered en SQL Server y Postgres).
    #[inline]
    pub fn new_v7() -> Self {
        Self { value: Uuid::now_v7().hyphenated().to_string(), _db: PhantomData }
    }

    /// Genera un `UUID v4` aleatorio.
    #[inline]
    pub fn new_v4() -> Self {
        Self { value: Uuid::new_v4().hyphenated().to_string(), _db: PhantomData }
    }

    #[inline]
    pub fn from_uuid(id: Uuid) -> Self {
        Self { value: id.hyphenated().to_string(), _db: PhantomData }
    }
}

#[cfg(feature = "uuid")]
impl Default for Cast<String, Uuid> {
    fn default() -> Self {
        Self::new_v7()
    }
}

#[cfg(feature = "uuid")]
impl Cast<Uuid, Uuid> {
    #[inline]
    pub fn new_v7() -> Self {
        Self::new(Uuid::now_v7())
    }

    #[inline]
    pub fn new_v4() -> Self {
        Self::new(Uuid::new_v4())
    }
}

impl<T> Cast<T, T> {
    #[inline]
    pub const fn new(value: T) -> Self {
        Self { value, _db: PhantomData }
    }
}

impl<T> From<T> for Cast<T, T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T, Db: DbRepr<T>> Cast<T, Db> {
    pub fn parse(value: T) -> Result<Self, AppError> {
        Db::validate(&value).map_err(|reason| AppError::invalid("id", reason))?;
        Ok(Self { value, _db: PhantomData })
    }

    /// Reconstruye `Cast<T, Db>` directamente desde el valor nativo leído del driver de BD.
    pub fn from_db(db_value: Db) -> Result<Self, AppError> {
        let value = Db::from_db(db_value).map_err(AppError::database)?;
        Ok(Self { value, _db: PhantomData })
    }

    #[inline]
    pub fn get(&self) -> &T {
        &self.value
    }

    #[inline]
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Devuelve el valor convertido al tipo nativo de la BD (`i64`, `String`, `ObjectId`, `Uuid`...).
    pub fn to_db(&self) -> Result<Db, AppError> {
        Db::to_db(&self.value).map_err(|reason| AppError::invalid("id", reason))
    }

    #[cfg(feature = "mongo")]
    pub(crate) fn to_bson(&self) -> Result<Bson, AppError>
    where
        Db: Serialize,
    {
        let db_val = self.to_db()?;
        with_db_mode(|| bson::serialize_to_bson(&db_val))
            .map_err(|e| AppError::database(format!("failed to encode value to BSON: {e}")))
    }
}

impl<T, Db> Deref for Cast<T, Db> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Clone, Db> Clone for Cast<T, Db> {
    fn clone(&self) -> Self {
        Self { value: self.value.clone(), _db: PhantomData }
    }
}

impl<T: Copy, Db> Copy for Cast<T, Db> {}

impl<T: PartialEq, Db> PartialEq for Cast<T, Db> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq, Db> Eq for Cast<T, Db> {}

impl<T: PartialOrd, Db> PartialOrd for Cast<T, Db> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: Ord, Db> Ord for Cast<T, Db> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl<T: Hash, Db> Hash for Cast<T, Db> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: fmt::Debug, Db> fmt::Debug for Cast<T, Db> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cast({:?})", self.value)
    }
}

impl<T: fmt::Display, Db> fmt::Display for Cast<T, Db> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T, Db> FromStr for Cast<T, Db>
where
    T: FromStr,
    Db: DbRepr<T>,
{
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parsed = s
            .trim()
            .parse::<T>()
            .map_err(|_| AppError::invalid("id", "invalid identifier format"))?;
        Self::parse(parsed)
    }
}

impl<Db: DbRepr<String>> PartialEq<&str> for Cast<String, Db> {
    fn eq(&self, other: &&str) -> bool {
        self.value == *other
    }
}

impl<T, Db> Serialize for Cast<T, Db>
where
    T: Serialize,
    Db: DbRepr<T> + Serialize,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if is_db_mode() {
            let db_val = Db::to_db(&self.value).map_err(serde::ser::Error::custom)?;
            db_val.serialize(serializer)
        } else {
            self.value.serialize(serializer)
        }
    }
}

impl<'de, T, Db> Deserialize<'de> for Cast<T, Db>
where
    T: Deserialize<'de>,
    Db: DbRepr<T> + Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if is_db_mode() {
            let db_val = Db::deserialize(deserializer)?;
            let value = Db::from_db(db_val).map_err(de::Error::custom)?;
            Ok(Self { value, _db: PhantomData })
        } else {
            let value = T::deserialize(deserializer)?;
            Db::validate(&value).map_err(de::Error::custom)?;
            Ok(Self { value, _db: PhantomData })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_and_generic_casts_work_without_mongo_feature() {
        let sql_id: Cast<i64> = Cast::from(42_i64);
        let copy = sql_id;
        assert_eq!(*copy, 42);
        assert_eq!(serde_json::to_string(&sql_id).expect("json"), "42");
        assert_eq!(sql_id.to_db().expect("to_db"), 42_i64);

        let str_id: Cast<String> = Cast::from("usr_custom_01".to_string());
        assert_eq!(serde_json::to_string(&str_id).expect("json"), "\"usr_custom_01\"");

        let bridged = Cast::<String, i64>::from_db(9_007_199_254_740_993_i64).expect("from_db");
        assert_eq!(serde_json::to_string(&bridged).expect("client json"), "\"9007199254740993\"");
        assert_eq!(
            with_db_mode(|| serde_json::to_string(&bridged)).expect("db mode"),
            "9007199254740993"
        );
        assert_eq!(bridged.to_db().expect("sql param"), 9_007_199_254_740_993_i64);
        assert!(serde_json::from_str::<Cast<String, i64>>("\"abc\"").is_err());
    }

    #[cfg(feature = "mongo")]
    #[test]
    fn string_to_objectid_bridges_json_and_bson_when_mongo_enabled() {
        let oid = ObjectId::parse_str("65f0a1b2c3d4e5f60718293a").expect("oid");
        let id: Cast<String, ObjectId> = Cast::from_oid(oid);

        assert_eq!(id.as_str(), "65f0a1b2c3d4e5f60718293a");
        assert_eq!(serde_json::to_string(&id).expect("json"), "\"65f0a1b2c3d4e5f60718293a\"");
        assert_eq!(id.to_bson().expect("bson"), Bson::ObjectId(oid));

        assert!(serde_json::from_str::<Cast<String, ObjectId>>("\"not-an-id\"").is_err());
        assert!(
            serde_json::from_str::<Cast<String, ObjectId>>(
                r#"{"$oid":"65f0a1b2c3d4e5f60718293a"}"#
            )
            .is_err()
        );

        let flex_oid: Cast<String, FlexibleId> =
            serde_json::from_str("\"65f0a1b2c3d4e5f60718293a\"").expect("flex oid");
        assert_eq!(flex_oid.to_bson().expect("bson"), Bson::ObjectId(oid));

        let flex_str: Cast<String, FlexibleId> =
            serde_json::from_str("\"MX-WH-01\"").expect("flex str");
        assert_eq!(flex_str.to_bson().expect("bson"), Bson::String("MX-WH-01".into()));
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_v7_and_v4_casts_bridge_string_and_native_uuid() {
        let id = Cast::<String, Uuid>::new_v7();
        assert_eq!(id.len(), 36);

        let native: Uuid = id.to_db().expect("native uuid");
        assert_eq!(native.get_version_num(), 7);

        let restored = Cast::<String, Uuid>::from_db(native).expect("from_db");
        assert_eq!(restored, id);

        let json = serde_json::to_string(&id).expect("json");
        let from_json: Cast<String, Uuid> = serde_json::from_str(&json).expect("decode");
        assert_eq!(from_json, id);

        assert!(serde_json::from_str::<Cast<String, Uuid>>("\"not-a-uuid\"").is_err());

        let direct_copy = Cast::<Uuid>::new_v7();
        let copied = direct_copy;
        assert_eq!(copied, direct_copy);
    }
}
