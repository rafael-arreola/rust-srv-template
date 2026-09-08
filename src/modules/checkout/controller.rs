use axum::{Router, extract::State, routing::post};
use std::sync::Arc;

use super::dto::{CheckoutInput, CheckoutOutput};
use super::workflow::CheckoutWorkflow;
use crate::core::error::AppError;
use crate::core::extractors::ValidatedBody;
use crate::core::response::ApiResponse;

pub fn routes(workflow: Arc<CheckoutWorkflow>) -> Router {
    Router::new().route("/", post(execute_checkout)).with_state(workflow)
}

#[tracing::instrument(skip_all)]
async fn execute_checkout(
    State(workflow): State<Arc<CheckoutWorkflow>>,
    ValidatedBody(input): ValidatedBody<CheckoutInput>,
) -> Result<ApiResponse<CheckoutOutput>, AppError> {
    let output = workflow.execute(input).await?;
    Ok(ApiResponse::created(output))
}
