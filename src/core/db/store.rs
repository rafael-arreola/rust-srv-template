//! Handle de persistencia que el composition root entrega a los módulos.
//!
//! Es la costura entre el servicio y el motor de datos: `app.rs`, `modules/mod.rs` y el
//! constructor de cada slice reciben un `Store` y nunca nombran el driver. Solo `repository.rs`
//! baja al handle concreto con [`Store::mongo`]. Cambiar de motor significa añadir aquí el handle
//! del nuevo backend, construirlo en `main.rs` (que hoy conecta con `MongoProvider`) y reescribir
//! los `repository.rs`/`document.rs` de cada slice; ningún otro archivo del árbol conoce la
//! diferencia.

use mongodb::Database;

#[derive(Clone)]
pub struct Store {
    mongo: Database,
}

impl Store {
    /// Envuelve la base MongoDB ya conectada y verificada por [`super::MongoProvider`].
    pub fn from_mongo(database: Database) -> Self {
        Self { mongo: database }
    }

    /// Handle del driver para los repositorios del backend MongoDB.
    pub fn mongo(&self) -> &Database {
        &self.mongo
    }
}

#[cfg(test)]
impl Store {
    /// Base efímera para tests de integración. `MONGO_TEST_URL` apunta a un MongoDB desechable y
    /// cada llamada recibe una base de nombre único, así los tests corren en paralelo sin pisarse.
    pub async fn connect_for_tests() -> Self {
        use mongodb::bson::oid::ObjectId;

        let url = std::env::var("MONGO_TEST_URL")
            .expect("MONGO_TEST_URL must point to a disposable MongoDB");
        let name = format!("test_{}", ObjectId::new().to_hex());
        let provider = super::MongoProvider::new("rust-srv-template-tests", &url, &name)
            .await
            .expect("connect to the test MongoDB");
        Self::from_mongo(provider.database())
    }

    pub async fn drop_for_tests(&self) {
        self.mongo.drop().await.expect("drop the test database");
    }
}
