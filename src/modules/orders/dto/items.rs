use serde::{Deserialize, Serialize};
use validator::Validate;

/// Contrato de entrada para los items subordinados de una orden.
#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct OrderItemInput {
    #[validate(length(min = 1, message = "Item name is required"))]
    pub name: String,

    #[validate(range(min = 1, message = "Quantity must be at least 1"))]
    pub quantity: u32,

    #[validate(range(min = 0.01, message = "Unit price must be positive"))]
    pub unit_price: f64,
}

/// Contrato de salida para los items subordinados de una orden.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderItemOutput {
    pub name: String,
    pub quantity: u32,
    pub unit_price: f64,
    pub total: f64,
}
