use axum::{Router, extract::State, routing::post};
use std::sync::Arc;

use super::dto::{CheckoutInput, CheckoutOutput};
use super::workflow::CheckoutWorkflow;
use crate::core::context::Context;
use crate::core::error::AppResult;
use crate::core::extractors::ValidatedBody;
use crate::core::response::ApiResponse;

pub fn routes(workflow: Arc<CheckoutWorkflow>) -> Router {
    Router::new().route("/", post(execute_checkout)).with_state(workflow)
}

#[tracing::instrument(skip_all)]
async fn execute_checkout(
    ctx: Context,
    State(workflow): State<Arc<CheckoutWorkflow>>,
    ValidatedBody(input): ValidatedBody<CheckoutInput>,
) -> AppResult<ApiResponse<CheckoutOutput>> {
    let output = workflow.execute(&ctx, input).await?;
    Ok(ApiResponse::created(output))
}
