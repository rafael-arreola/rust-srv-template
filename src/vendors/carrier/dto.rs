use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateShipmentInput {
    pub destination_address: String,
    pub recipient_name: String,
    pub recipient_phone: Option<String>,
    pub package_weight_kg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShipmentResponse {
    pub tracking_number: String,
    pub carrier_status: String,
    pub estimated_delivery: Option<String>,
}
