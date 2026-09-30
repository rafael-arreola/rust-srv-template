use std::sync::Arc;

use super::dto::{CreateShipmentInput, ShipmentRequest, ShipmentResponse};
use crate::core::error::{AppError, AppResult};
use crate::core::net::HttpClient;

/// Cliente HTTP instrumentado para interactuar con la API del Carrier externo.
#[derive(Clone)]
pub struct CarrierClient {
    client: Arc<HttpClient>,
    base_url: String,
    api_key: String,
}

impl CarrierClient {
    /// La API key es obligatoria: un vendor autenticado sin credencial no degrada a peticiones
    /// anónimas. Una key vacía es un error de arranque; léela con `require_env` al cablearlo.
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
        // Solo el status y el tamaño del cuerpo llegan al log: los carriers suelen devolver la
        // solicitud dentro del error, y esa solicitud trae nombre, teléfono y dirección del
        // destinatario.
        if !status.is_success() {
            let detail = match response.bytes().await {
                Ok(body) => format!("HTTP {status}, error body of {} bytes", body.len()),
                Err(e) => format!("HTTP {status}, error body unreadable: {e}"),
            };
            return Err(AppError::external_service("Carrier", detail));
        }

        // Sin `{e}`: el mensaje de serde repite el valor que no encajó (`invalid type: string
        // "Juan Pérez"`), y la respuesta trae datos del destinatario.
        response.json::<ShipmentResponse>().await.map_err(|_| {
            AppError::external_service("Carrier", "response body does not match ShipmentResponse")
        })
    }
}
