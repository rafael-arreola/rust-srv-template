use reqwest_middleware::ClientWithMiddleware;

use super::dto::{CreateShipmentInput, ShipmentResponse};
use crate::core::error::{AppError, AppResult};

/// Cliente HTTP instrumentado para interactuar con la API del Carrier externo.
#[derive(Clone)]
pub struct CarrierClient {
    client: ClientWithMiddleware,
    base_url: String,
    api_key: Option<String>,
}

impl CarrierClient {
    pub fn new(
        client: ClientWithMiddleware,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self { client, base_url: base_url.into().trim_end_matches('/').to_string(), api_key }
    }

    #[tracing::instrument(skip_all, fields(recipient = %req.recipient_name))]
    pub async fn create_shipment(&self, req: &CreateShipmentInput) -> AppResult<ShipmentResponse> {
        let url = format!("{}/v1/shipments", self.base_url);

        let mut builder = self.client.post(&url).json(req);
        if let Some(key) = &self.api_key {
            builder = builder.header("Authorization", format!("Bearer {key}"));
        }

        let response = builder
            .send()
            .await
            .map_err(|e| AppError::external_service("Carrier", e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::external_service(
                "Carrier",
                format!("HTTP {status}: {error_text}"),
            ));
        }

        response.json::<ShipmentResponse>().await.map_err(|e| {
            AppError::external_service("Carrier", format!("Failed to parse response: {e}"))
        })
    }
}
