use chrono::Utc;

use crate::core::error::{AppError, AppResult};
use crate::shared::Money;

/// Estado de una orden en el dominio. Su forma almacenada es `OrderStatusDocument`; `as_str` es el
/// texto que el contrato HTTP publica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Created,
}

impl OrderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
        }
    }
}

/// Línea de una orden. `line_total` se calcula una sola vez, con aritmética comprobada, al
/// construirla: después es un hecho registrado, igual que el precio unitario que lo originó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderItem {
    name: String,
    quantity: u32,
    unit_price: Money,
    line_total: Money,
}

impl OrderItem {
    pub fn new(name: String, quantity: u32, unit_price: Money) -> AppResult<Self> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::invalid("items", "item name is required"));
        }
        if quantity == 0 {
            return Err(AppError::invalid("items", "item quantity must be at least 1"));
        }
        if !unit_price.is_positive() {
            return Err(AppError::invalid("items", "item unit_price must be positive"));
        }
        let line_total = unit_price.checked_mul(quantity)?;
        Ok(Self { name, quantity, unit_price, line_total })
    }

    /// Rehidrata una línea leída del almacenamiento sin recalcular: los invariantes ya se
    /// aplicaron al escribirla.
    pub fn restore(name: String, quantity: u32, unit_price: Money, line_total: Money) -> Self {
        Self { name, quantity, unit_price, line_total }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn quantity(&self) -> u32 {
        self.quantity
    }

    pub fn unit_price(&self) -> Money {
        self.unit_price
    }

    pub fn line_total(&self) -> Money {
        self.line_total
    }
}

/// Entidad de dominio pura `Order`.
///
/// Desacoplada de MongoDB, BSON y tipos de infraestructura. `user_id` es un identificador opaco
/// hacia `users`: la orden no conoce nada más del usuario y no guarda copias de sus datos.
/// `idempotency_key` la elige el cliente; junto con `user_id` identifica una única solicitud de
/// compra, de modo que un reintento no registra una segunda orden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    id: String,
    user_id: String,
    idempotency_key: String,
    items: Vec<OrderItem>,
    total: Money,
    status: OrderStatus,
    created_at: chrono::DateTime<Utc>,
}

impl Order {
    /// Orden aún no persistida: el identificador lo asigna el repositorio (ver [`Order::with_id`]).
    /// Exige al menos una línea y una única moneda; el total es la suma comprobada de las líneas.
    /// `now` lo lee el service: la entidad no consulta el reloj.
    pub fn new(
        user_id: String,
        idempotency_key: String,
        items: Vec<OrderItem>,
        now: chrono::DateTime<Utc>,
    ) -> AppResult<Self> {
        if user_id.trim().is_empty() {
            return Err(AppError::required("user_id"));
        }
        if idempotency_key.trim().is_empty() {
            return Err(AppError::required("idempotency_key"));
        }
        let total = Money::try_sum(items.iter().map(OrderItem::line_total))?
            .ok_or_else(|| AppError::invalid("items", "an order needs at least one item"))?;

        Ok(Self {
            id: String::new(),
            user_id,
            idempotency_key,
            items,
            total,
            status: OrderStatus::Created,
            created_at: now,
        })
    }

    /// Rehidrata una orden leída del almacenamiento tal como se registró.
    pub fn restore(
        id: String,
        user_id: String,
        idempotency_key: String,
        items: Vec<OrderItem>,
        total: Money,
        status: OrderStatus,
        created_at: chrono::DateTime<Utc>,
    ) -> Self {
        Self { id, user_id, idempotency_key, items, total, status, created_at }
    }

    /// `true` si `other` describe la misma compra: mismo usuario, misma clave y mismas líneas.
    /// Distingue un reintento legítimo de una clave reutilizada para otra compra.
    pub fn is_same_request(&self, other: &Order) -> bool {
        self.user_id == other.user_id
            && self.idempotency_key == other.idempotency_key
            && self.items == other.items
    }

    /// Identificador asignado por el almacenamiento tras la inserción.
    pub fn with_id(mut self, id: String) -> Self {
        self.id = id;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn items(&self) -> &[OrderItem] {
        &self.items
    }

    pub fn total(&self) -> Money {
        self.total
    }

    pub fn status(&self) -> OrderStatus {
        self.status
    }

    pub fn created_at(&self) -> chrono::DateTime<Utc> {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::Currency;

    fn at(seconds: i64) -> chrono::DateTime<Utc> {
        chrono::DateTime::from_timestamp(seconds, 0).expect("valid timestamp")
    }

    fn mxn(amount: i64) -> Money {
        Money::new(amount, Currency::Mxn)
    }

    fn item(name: &str, quantity: u32, unit_price: Money) -> OrderItem {
        OrderItem::new(name.into(), quantity, unit_price).expect("valid item")
    }

    #[test]
    fn line_total_is_price_times_quantity() {
        assert_eq!(item("Mouse", 3, mxn(1999)).line_total(), mxn(5997));
    }

    #[test]
    fn invalid_items_are_rejected() {
        assert!(OrderItem::new("   ".into(), 1, mxn(100)).is_err());
        assert!(OrderItem::new("Mouse".into(), 0, mxn(100)).is_err());
        assert!(OrderItem::new("Mouse".into(), 1, mxn(0)).is_err());
    }

    #[test]
    fn total_is_the_checked_sum_of_lines() {
        let order = Order::new(
            "u1".into(),
            "k1".into(),
            vec![item("A", 2, mxn(500)), item("B", 1, mxn(250))],
            at(1_000),
        )
        .expect("valid order");
        assert_eq!(order.total(), mxn(1250));
        assert_eq!((order.status(), order.created_at()), (OrderStatus::Created, at(1_000)));
    }

    #[test]
    fn order_needs_items_of_a_single_currency() {
        assert!(Order::new("u1".into(), "k1".into(), Vec::new(), at(0)).is_err());
        let mixed = vec![item("A", 1, mxn(100)), item("B", 1, Money::new(100, Currency::Clp))];
        assert!(Order::new("u1".into(), "k1".into(), mixed, at(0)).is_err());
    }

    #[test]
    fn same_request_ignores_id_and_instant_but_not_items() {
        let a = Order::new("u1".into(), "k1".into(), vec![item("A", 1, mxn(100))], at(1))
            .expect("valid");
        let replay = Order::new("u1".into(), "k1".into(), vec![item("A", 1, mxn(100))], at(9))
            .expect("valid");
        let other = Order::new("u1".into(), "k1".into(), vec![item("A", 2, mxn(100))], at(1))
            .expect("valid");
        assert!(a.clone().with_id("x".into()).is_same_request(&replay));
        assert!(!a.is_same_request(&other));
    }
}
