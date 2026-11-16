#[cfg(feature = "mongo")]
pub mod mongo;
pub mod net;
pub mod page;
#[cfg(feature = "redis")]
pub mod redis;

#[cfg(feature = "mongo")]
pub use self::mongo::{Mongo, MongoDb};
#[cfg(feature = "redis")]
pub use self::redis::{Redis, RedisProvider};
pub use net::{Http, HttpClient};
pub use page::QueryResult;

/// Primitivas directas del driver y puentes BSON confinadas a `engine::db::*` para `service/src/repos/`.
pub mod db {
    #[cfg(feature = "redis")]
    pub use ::redis::AsyncCommands;
    #[cfg(feature = "mongo")]
    pub use futures::TryStreamExt;
    #[cfg(feature = "mongo")]
    pub use mongodb::{
        Client, ClientSession, Collection, Database, IndexModel,
        bson::{self, Bson, Document, doc},
        options::{
            CollectionOptions, IndexOptions, ReadPreference, ReturnDocument, SelectionCriteria,
        },
    };

    #[cfg(feature = "mongo")]
    pub use super::mongo::{
        BsonExt, Mongo, MongoDb, decode_entity, encode_entity, escape_regex, is_duplicate_key,
    };
    pub use super::page::QueryResult;
    #[cfg(feature = "redis")]
    pub use super::redis::{Redis, RedisProvider};
}
