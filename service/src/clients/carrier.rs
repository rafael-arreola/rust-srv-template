use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::models::{Country, Kilograms};
use engine::{AppError, AppResult, HttpClient};

#[derive(Debug, Clone)]
pub struct CreateShipmentInput {
    pub destination_country: Country,
    pub destination_address: String,
    pub recipient_name: String,
    pub recipient_phone: Option<String>,
    pub package_weight: Kilograms,
}

#[derive(Debug, Clone, Serialize)]
struct ShipmentRequest {
    destination_country: &'static str,
    destination_address: String,
    recipient_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    recipient_phone: Option<String>,
    package_weight_kg: f64,
}

impl From<&CreateShipmentInput> for ShipmentRequest {
    fn from(input: &CreateShipmentInput) -> Self {
        let code = match input.destination_country {
            Country::Mx => "MEX",
            Country::Co => "COL",
            Country::Pe => "PER",
            Country::Cl => "CHL",
        };
        Self {
            destination_country: code,
            destination_address: input.destination_address.clone(),
            recipient_name: input.recipient_name.clone(),
            recipient_phone: input.recipient_phone.clone(),
            package_weight_kg: input.package_weight.value(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShipmentResponse {
    pub tracking_number: String,
    pub carrier_status: String,
    pub estimated_delivery: Option<String>,
}

#[derive(Clone)]
pub struct CarrierClient {
    client: Arc<HttpClient>,
    base_url: String,
    api_key: String,
}

impl CarrierClient {
    pub fn new(
        client: Arc<HttpClient>,
        base_url: impl Into<String>,
        api_key: String,
    ) -> AppResult<Self> {
        if api_key.trim().is_empty() {
            return Err(AppError::internal("Carrier API key is empty"));
        }
        Ok(Self { client, base_url: base_url.into().trim_end_matches('/').to_string(), api_key })
    }

    #[tracing::instrument(skip_all)]
    pub async fn create_shipment(
        &self,
        input: &CreateShipmentInput,
    ) -> AppResult<ShipmentResponse> {
        let url = format!("{}/v1/shipments", self.base_url);
        let payload = ShipmentRequest::from(input);

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::external_service("Carrier", e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let detail = match response.bytes().await {
                Ok(body) => format!("HTTP {status}, error body of {} bytes", body.len()),
                Err(e) => format!("HTTP {status}, error body unreadable: {e}"),
            };
            return Err(AppError::external_service("Carrier", detail));
        }

        response.json::<ShipmentResponse>().await.map_err(|_| {
            AppError::external_service("Carrier", "response body does not match ShipmentResponse")
        })
    }
}
