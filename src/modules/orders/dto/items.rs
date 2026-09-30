use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::shared::Money;

/// Contrato de entrada para las líneas de una orden. El precio viaja como `Money`
/// (`{ "amount": 1999, "currency": "MXN" }`): la positividad y la moneda única se validan en
/// `OrderItem::new` y `Money::try_sum`.
#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct OrderItemInput {
    #[validate(length(min = 1, message = "Item name is required"))]
    pub name: String,

    #[validate(range(min = 1, message = "Quantity must be at least 1"))]
    pub quantity: u32,

    pub unit_price: Money,
}

/// Contrato de salida para las líneas de una orden.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderItemOutput {
    pub name: String,
    pub quantity: u32,
    pub unit_price: Money,
    pub total: Money,
}
