use mongodb::bson::oid::ObjectId;
use mongodb::options::{IndexOptions, ReadPreference, SelectionCriteria};
use mongodb::{Collection, IndexModel, bson::doc};

use super::document::OrderDocument;
use super::entity::Order;
use crate::core::db::Store;
use crate::core::db::mongo::is_duplicate_key;
use crate::core::error::{AppError, AppResult};

/// Resultado de registrar una orden bajo su clave idempotente.
#[derive(Debug, Clone)]
pub enum PlaceOutcome {
    Inserted(Order),
    /// Ya existía una orden con el mismo `(user_id, idempotency_key)`; se devuelve tal como quedó
    /// registrada para que el service decida si es un reintento o una clave reutilizada.
    AlreadyPlaced(Order),
}

#[derive(Clone)]
pub struct OrderRepository {
    collection: Collection<OrderDocument>,
}

impl OrderRepository {
    pub async fn new(store: &Store) -> AppResult<Self> {
        let repo = Self { collection: store.mongo().collection::<OrderDocument>("orders") };
        repo.create_indexes().await?;
        Ok(repo)
    }

    /// El índice único `(user_id, idempotency_key)` es quien arbitra la carrera entre dos
    /// reintentos simultáneos: no hay lectura previa, gana la primera inserción. Todo documento
    /// lleva la clave; una colección anterior a este esquema requiere reset (borrar la colección:
    /// `OrderRepository::new` recrea los índices vigentes).
    async fn create_indexes(&self) -> AppResult<()> {
        let index = IndexModel::builder()
            .keys(doc! { "user_id": 1, "idempotency_key": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("orders_user_idempotency_unique_idx".to_string())
                    .build(),
            )
            .build();

        self.collection.create_index(index).await?;

        tracing::info!("Orders collection indexes verified");
        Ok(())
    }

    /// La relectura tras la colisión va al primario: con `readPreference=secondary*` en la URI,
    /// una réplica atrasada aún no tendría la orden ganadora.
    #[tracing::instrument(skip_all)]
    pub async fn place(&self, order: &Order) -> AppResult<PlaceOutcome> {
        let id = ObjectId::new();
        let document = OrderDocument::from_domain(id, order)?;

        match self.collection.insert_one(&document).await {
            Ok(_) => Ok(PlaceOutcome::Inserted(order.clone().with_id(id.to_hex()))),
            Err(err) if is_duplicate_key(&err) => {
                let filter = doc! {
                    "user_id": document.user_id,
                    "idempotency_key": order.idempotency_key(),
                };

                let existing = self
                    .collection
                    .find_one(filter)
                    .selection_criteria(SelectionCriteria::ReadPreference(ReadPreference::Primary))
                    .await?
                    .ok_or_else(|| {
                        AppError::database(
                            "duplicate idempotency key reported but no order matches it",
                        )
                    })?;
                Ok(PlaceOutcome::AlreadyPlaced(existing.into_domain()))
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Un identificador sin forma de `ObjectId` no puede existir en la colección: es `None`, no
    /// un 400, para que el formato de almacenamiento no asome en el contrato HTTP.
    #[tracing::instrument(skip_all)]
    pub async fn find_by_id(&self, id: &str) -> AppResult<Option<Order>> {
        let Ok(oid) = ObjectId::parse_str(id) else {
            return Ok(None);
        };

        let filter = doc! { "_id": oid };

        let document = self.collection.find_one(filter).await?;

        Ok(document.map(OrderDocument::into_domain))
    }
}
