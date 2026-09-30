use serde::{Deserialize, Serialize};

use crate::shared::Country;

/// Solicitud de envío en el vocabulario del negocio. Es lo que un módulo entrega al vendor.
#[derive(Debug, Clone)]
pub struct CreateShipmentInput {
    pub destination_country: Country,
    pub destination_address: String,
    pub recipient_name: String,
    pub recipient_phone: Option<String>,
    pub package_weight_kg: f64,
}

/// Cuerpo que viaja al proveedor: su contrato, no el nuestro. El `Country` del negocio se
/// traduce aquí al código que el proveedor entiende (`MX` → `MEX`) y nunca se le envía la
/// representación interna.
#[derive(Debug, Clone, Serialize)]
pub struct ShipmentRequest {
    pub destination_country: &'static str,
    pub destination_address: String,
    pub recipient_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_phone: Option<String>,
    pub package_weight_kg: f64,
}

impl From<&CreateShipmentInput> for ShipmentRequest {
    fn from(input: &CreateShipmentInput) -> Self {
        Self {
            destination_country: provider_country_code(input.destination_country),
            destination_address: input.destination_address.clone(),
            recipient_name: input.recipient_name.clone(),
            recipient_phone: input.recipient_phone.clone(),
            package_weight_kg: input.package_weight_kg,
        }
    }
}

/// Códigos de país del proveedor (ISO 3166-1 alpha-3). La tabla vive en el vendor porque es el
/// proveedor, y no el negocio, quien decide su vocabulario.
fn provider_country_code(country: Country) -> &'static str {
    match country {
        Country::Mx => "MEX",
        Country::Co => "COL",
        Country::Pe => "PER",
        Country::Cl => "CHL",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShipmentResponse {
    pub tracking_number: String,
    pub carrier_status: String,
    pub estimated_delivery: Option<String>,
}
