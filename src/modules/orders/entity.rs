use chrono::{DateTime, Utc};

/// Item de orden como concepto de valor/entidad de dominio pura.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderItem {
    pub name: String,
    pub quantity: u32,
    pub unit_price: f64,
}

impl OrderItem {
    pub fn new(name: String, quantity: u32, unit_price: f64) -> Self {
        Self { name, quantity, unit_price }
    }

    pub fn total(&self) -> f64 {
        self.quantity as f64 * self.unit_price
    }
}

/// Entidad de dominio pura `Order`.
/// Desacoplada de MongoDB, BSON y tipos de infraestructura.
/// Mantiene `user_id: String` como identificador opaco para evitar acoplamiento
/// o dependencias cíclicas con el dominio de usuarios.
#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub items: Vec<OrderItem>,
    pub total_amount: f64,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

impl Order {
    pub fn new(user_id: String, user_name: String, items: Vec<OrderItem>) -> Self {
        let total_amount = items.iter().map(|item| item.total()).sum();
        Self {
            id: String::new(),
            user_id,
            user_name,
            items,
            total_amount,
            status: "CREATED".to_string(),
            created_at: Utc::now(),
        }
    }
}
