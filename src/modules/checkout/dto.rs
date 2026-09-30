use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::shared::Money;

/// Línea de compra en el contrato propio de `checkout`. Se declara aquí, y no como alias del DTO
/// de `orders`, para que un cambio en el contrato HTTP de un módulo no altere el del otro sin que
/// nadie lo toque; el workflow la traduce al contrato de `orders`.
// `Serialize` lo exige el validador `length` sobre `Vec<CheckoutItemInput>`, que adjunta el valor
// al error de validación.
#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct CheckoutItemInput {
    #[validate(length(min = 1, message = "Item name is required"))]
    pub name: String,

    #[validate(range(min = 1, message = "Quantity must be at least 1"))]
    pub quantity: u32,

    pub unit_price: Money,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CheckoutInput {
    #[validate(length(min = 1, message = "user_id is required"))]
    pub user_id: String,

    /// Clave elegida por el cliente (un UUID por intento de compra). Reenviar el mismo body con la
    /// misma clave tras un timeout devuelve la orden ya registrada en vez de crear otra.
    #[validate(length(min = 1, max = 128, message = "idempotency_key must be 1-128 characters"))]
    pub idempotency_key: String,

    #[validate(length(min = 1, message = "At least one item is required"), nested)]
    pub items: Vec<CheckoutItemInput>,
}

/// Resultado consolidado del checkout. Devuelve identificadores y el hecho registrado por
/// `orders`; los datos del usuario los sirve `users` y no se copian aquí.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckoutOutput {
    pub order_id: String,
    pub user_id: String,
    pub total: Money,
    pub items_count: usize,
    pub status: String,
}
