use std::future::Future;
use std::ops::Deref;
use std::time::Duration;

use mongodb::{
    Client, ClientSession, Database,
    bson::{self, Document, doc},
    error::{Error as MongoError, ErrorKind as MongoErrorKind, WriteFailure},
    options::{Acknowledgment, ClientOptions, ReadPreference, SelectionCriteria},
};
use serde::{Serialize, de::DeserializeOwned};
use tokio::time::timeout;

use crate::{
    cast::{Cast, DbRepr, with_db_mode},
    error::{AppError, AppResult},
    registry::Registry,
    time::UtcDateTime,
};

static MONGO_REGISTRY: Registry<MongoDb> = Registry::new("Mongo");

/// Conexión registrada a una base de datos MongoDB (`Deref<Target = Database>` + `Client` para sesiones).
#[derive(Clone)]
pub struct MongoDb {
    client: Client,
    db: Database,
}

impl MongoDb {
    pub fn from_parts(client: Client, db: Database) -> Self {
        Self { client, db }
    }

    #[inline]
    pub fn client(&self) -> &Client {
        &self.client
    }

    #[inline]
    pub fn database(&self) -> &Database {
        &self.db
    }

    pub async fn start_session(&self) -> AppResult<ClientSession> {
        Ok(self.client.start_session().await?)
    }
}

impl Deref for MongoDb {
    type Target = Database;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

#[cfg(any(test, feature = "testing"))]
#[allow(clippy::expect_used)]
impl MongoDb {
    pub async fn connect_for_tests() -> Self {
        use mongodb::bson::oid::ObjectId;

        let url = std::env::var("MONGO_TEST_URL")
            .expect("MONGO_TEST_URL must point to a disposable MongoDB");
        let name = format!("test_{}", ObjectId::new().to_hex());
        connect("rust-srv-template-tests", &url, &name).await.expect("connect to the test MongoDB")
    }

    pub async fn drop_for_tests(&self) {
        self.db.drop().await.expect("drop the test database");
    }
}

/// Facade global de instancias MongoDB nombradas (`Mongo::new(url, db, "default").await?`, `Mongo::get("default")?`).
pub struct Mongo;

impl Mongo {
    pub const DEFAULT: &'static str = "default";

    #[allow(clippy::new_ret_no_self)]
    pub async fn new(url: &str, database: &str, instance: &str) -> AppResult<MongoDb> {
        let handle = connect(instance, url, database).await?;
        MONGO_REGISTRY.register(instance, handle.clone());
        Ok(handle)
    }

    #[track_caller]
    pub fn get(instance: &str) -> AppResult<MongoDb> {
        MONGO_REGISTRY.get(instance)
    }

    pub fn try_get(instance: &str) -> Option<MongoDb> {
        MONGO_REGISTRY.try_get(instance)
    }

    pub async fn scope<F: Future>(instance: &str, db: MongoDb, fut: F) -> F::Output {
        MONGO_REGISTRY.scope(instance, db, fut).await
    }
}

#[tracing::instrument(skip_all, fields(dependency = "mongodb"))]
async fn connect(app_name: &str, mongo_url: &str, mongo_db: &str) -> AppResult<MongoDb> {
    let mut client_options = timeout(Duration::from_secs(10), ClientOptions::parse(mongo_url))
        .await
        .map_err(|_| AppError::timeout("MongoDB configuration discovery"))?
        .map_err(|e| AppError::database(format!("Failed to parse MongoDB URI: {e}")))?;

    client_options.app_name = Some(app_name.to_string());
    client_options.connect_timeout = Some(Duration::from_secs(3));
    client_options.server_selection_timeout = Some(Duration::from_secs(10));
    client_options.selection_criteria.get_or_insert_with(|| {
        SelectionCriteria::ReadPreference(ReadPreference::SecondaryPreferred { options: None })
    });
    client_options.write_concern.get_or_insert_default().w.get_or_insert(Acknowledgment::Majority);

    let client = Client::with_options(client_options)
        .map_err(|e| AppError::database(format!("Failed to initialize MongoDB client: {e}")))?;
    let db = client.database(mongo_db);
    db.run_command(doc! { "ping": 1 })
        .await
        .map_err(|e| AppError::database(format!("Failed to ping MongoDB: {e}")))?;

    tracing::info!("Connected to MongoDB database: {}", mongo_db);
    Ok(MongoDb { client, db })
}

const DUPLICATE_KEY_CODE: i32 = 11000;

impl From<MongoError> for AppError {
    fn from(err: MongoError) -> Self {
        if is_duplicate_key(&err) {
            return AppError::already_exists("Resource", "a unique field is already in use")
                .with_internal("unique index violation (E11000)");
        }
        AppError::database(err.to_string())
    }
}

pub fn is_duplicate_key(err: &MongoError) -> bool {
    match &*err.kind {
        MongoErrorKind::Write(WriteFailure::WriteError(write_error)) => {
            write_error.code == DUPLICATE_KEY_CODE
        }
        MongoErrorKind::Command(command_error) => command_error.code == DUPLICATE_KEY_CODE,
        MongoErrorKind::InsertMany(insert_error) => insert_error
            .write_errors
            .as_ref()
            .is_some_and(|errors| errors.iter().any(|e| e.code == DUPLICATE_KEY_CODE)),
        MongoErrorKind::BulkWrite(bulk_error) => {
            bulk_error.write_errors.values().any(|e| e.code == DUPLICATE_KEY_CODE)
        }
        _ => false,
    }
}

/// Escapa metacaracteres para búsquedas literales seguras con `$regex` en MongoDB.
pub fn escape_regex(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// Conversión hacia primitivos BSON confinada exclusivamente a `engine::db` (repositorios).
pub trait BsonExt {
    type Output;
    fn to_bson(&self) -> Self::Output;
}

impl<T, Db> BsonExt for Cast<T, Db>
where
    Db: DbRepr<T> + Serialize,
{
    type Output = AppResult<bson::Bson>;

    #[inline]
    fn to_bson(&self) -> Self::Output {
        Cast::to_bson(self)
    }
}

impl BsonExt for UtcDateTime {
    type Output = bson::DateTime;

    #[inline]
    fn to_bson(&self) -> Self::Output {
        UtcDateTime::to_bson(*self)
    }
}

/// Serializa un modelo o struct hacia `Document` activando `with_db_mode` y mapeando `id` -> `_id`.
pub fn encode_entity<E: Serialize>(entity: &E) -> AppResult<Document> {
    let mut doc = with_db_mode(|| bson::serialize_to_document(entity))
        .map_err(|e| AppError::database(format!("BSON encode error: {e}")))?;
    if let Some(id) = doc.remove("id") {
        doc.insert("_id", id);
    }
    Ok(doc)
}

/// Deserializa un `Document` (incluyendo proyecciones parciales) hacia `E` activando `with_db_mode`
/// y mapeando `_id` -> `id`.
pub fn decode_entity<E: DeserializeOwned>(mut doc: Document) -> AppResult<E> {
    if !doc.contains_key("id")
        && let Some(id) = doc.remove("_id")
    {
        doc.insert("id", id);
    }
    with_db_mode(|| bson::deserialize_from_document(doc))
        .map_err(|e| AppError::database(format!("BSON decode error: {e}")))
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Bson, oid::ObjectId};
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{Cast, UtcDateTime};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct MongoEntity {
        pub id: Cast<String, ObjectId>,
        pub legacy_seq: Cast<i64>,
        pub external_ref: Cast<String>,
        pub amount_cents: i64,
        pub created_at: UtcDateTime,
    }

    #[test]
    fn encodes_all_cast_strategies_and_decodes_legacy_string_dates() {
        let oid = ObjectId::parse_str("65f0a1b2c3d4e5f60718293a").expect("oid");
        let created_at = UtcDateTime::from_chrono(
            chrono::DateTime::from_timestamp_millis(1_700_000_000_000).expect("ms"),
        );

        let entity = MongoEntity {
            id: Cast::from_oid(oid),
            legacy_seq: Cast::from(99_i64),
            external_ref: Cast::from("ext_01".to_string()),
            amount_cents: 1999,
            created_at,
        };

        let doc = encode_entity(&entity).expect("encode");
        assert_eq!(doc.get("_id"), Some(&Bson::ObjectId(oid)));
        assert_eq!(doc.get("legacy_seq"), Some(&Bson::Int64(99)));
        assert_eq!(doc.get("external_ref"), Some(&Bson::String("ext_01".into())));
        assert_eq!(doc.get("created_at"), Some(&Bson::DateTime(created_at.to_bson())));

        let roundtrip: MongoEntity = decode_entity(doc).expect("decode");
        assert_eq!(roundtrip, entity);

        // Legacy document where an older service stored `created_at` as an RFC 3339 string
        let legacy_doc = doc! {
            "_id": oid,
            "legacy_seq": 99_i64,
            "external_ref": "ext_01",
            "amount_cents": 1999_i64,
            "created_at": "2023-11-14T22:13:20.000Z",
        };
        let from_legacy: MongoEntity = decode_entity(legacy_doc).expect("legacy decode");
        assert_eq!(from_legacy, entity);

        assert_eq!(escape_regex("a+b*(c)"), r"a\+b\*\(c\)");
    }
}
