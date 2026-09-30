use mongodb::bson;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use super::entity::{Order, OrderItem, OrderStatus};
use crate::core::error::{AppError, AppResult};
use crate::shared::Money;

/// Estado tal como lo guarda la colección: el string estable en `SCREAMING_SNAKE_CASE`. Un valor
/// desconocido falla al deserializar y llega como error de base de datos: nunca una orden en un
/// estado inventado. Por lo mismo, una variante renombrada o eliminada deja ilegibles los
/// documentos que la guardan: para retirarla se conserva. El `match` exhaustivo de los dos `From`
/// obliga a mapear toda variante nueva.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatusDocument {
    Created,
}

impl From<OrderStatus> for OrderStatusDocument {
    fn from(status: OrderStatus) -> Self {
        match status {
            OrderStatus::Created => Self::Created,
        }
    }
}

impl From<OrderStatusDocument> for OrderStatus {
    fn from(status: OrderStatusDocument) -> Self {
        match status {
            OrderStatusDocument::Created => Self::Created,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderItemDocument {
    pub name: String,
    pub quantity: u32,
    pub unit_price: Money,
    pub line_total: Money,
}

impl OrderItemDocument {
    pub fn from_domain(item: &OrderItem) -> Self {
        Self {
            name: item.name().to_string(),
            quantity: item.quantity(),
            unit_price: item.unit_price(),
            line_total: item.line_total(),
        }
    }

    pub fn into_domain(self) -> OrderItem {
        OrderItem::restore(self.name, self.quantity, self.unit_price, self.line_total)
    }
}

/// Esquema de almacenamiento en MongoDB para `Order`.
///
/// Es la representación de la colección, con los tipos del motor: `_id` y `user_id` (referencia a
/// la colección `users`) son `ObjectId`, las fechas son `bson::DateTime` y el estado es
/// [`OrderStatusDocument`]. `Money` se persiste como subdocumento `{ amount, currency }`. La
/// entidad expone lo mismo con tipos de dominio: `user_id` como `String` opaco, fechas `chrono` y
/// `OrderStatus`. Privado al slice: solo `OrderRepository` lo importa.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderDocument {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub user_id: ObjectId,
    pub idempotency_key: String,
    pub items: Vec<OrderItemDocument>,
    pub total: Money,
    pub status: OrderStatusDocument,
    pub created_at: bson::DateTime,
}

impl OrderDocument {
    /// Documento listo para insertar bajo el `_id` que el repositorio acaba de generar.
    ///
    /// La entidad guarda `user_id` como identificador opaco; aquí se eleva al `ObjectId` de la
    /// colección `users`. Uno que no tenga esa forma no puede referenciar a ningún usuario.
    pub fn from_domain(id: ObjectId, order: &Order) -> AppResult<Self> {
        let user_id = ObjectId::parse_str(order.user_id())
            .map_err(|_| AppError::invalid("user_id", "not a valid user identifier"))?;

        Ok(Self {
            id,
            user_id,
            idempotency_key: order.idempotency_key().to_string(),
            items: order.items().iter().map(OrderItemDocument::from_domain).collect(),
            total: order.total(),
            status: order.status().into(),
            created_at: bson::DateTime::from_chrono(order.created_at()),
        })
    }

    pub fn into_domain(self) -> Order {
        Order::restore(
            self.id.to_hex(),
            self.user_id.to_hex(),
            self.idempotency_key,
            self.items.into_iter().map(OrderItemDocument::into_domain).collect(),
            self.total,
            self.status.into(),
            self.created_at.to_chrono(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::Currency;

    #[test]
    fn status_is_stored_as_its_stable_string() {
        let stored = bson::serialize_to_bson(&OrderStatusDocument::Created).expect("serializes");
        assert_eq!(stored, bson::Bson::String("CREATED".into()));
    }

    #[test]
    fn unknown_stored_status_fails_instead_of_defaulting() {
        let result = bson::deserialize_from_bson::<OrderStatusDocument>(bson::Bson::String(
            "SHIPPED".into(),
        ));
        assert!(result.is_err());
    }

    #[test]
    fn user_id_that_is_not_an_object_id_cannot_be_stored() {
        let item =
            OrderItem::new("A".into(), 1, Money::new(100, Currency::Mxn)).expect("valid item");
        let now = chrono::DateTime::from_timestamp(0, 0).expect("valid timestamp");
        let order =
            Order::new("not-an-id".into(), "k1".into(), vec![item], now).expect("valid order");
        let err = OrderDocument::from_domain(ObjectId::new(), &order).expect_err("must not store");
        assert_eq!(err.code(), "INVALID_INPUT");
    }
}
