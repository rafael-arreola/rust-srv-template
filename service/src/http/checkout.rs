use engine::web::{ReplyExt, Router, State, Status, StatusCode, ValidatedBody, post};
use engine::{AppResult, Context};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::bootstrap::AppContext;
use crate::models::{Money, Order, OrderId, OrderItem, OrderStatus, UserId};
use crate::services::{CheckoutService, PlaceCheckout};

// ── FORM REQUESTS & JSON RESOURCES ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CheckoutItemRequest {
    #[validate(length(min = 1, message = "Item name is required"))]
    pub name: String,

    #[validate(range(min = 1, message = "Quantity must be at least 1"))]
    pub quantity: u32,

    pub unit_price: Money,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CheckoutRequest {
    pub user_id: UserId,

    #[validate(length(min = 1, max = 128, message = "idempotency_key must be 1-128 characters"))]
    pub idempotency_key: String,

    #[validate(length(min = 1, message = "At least one item is required"), nested)]
    pub items: Vec<CheckoutItemRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutResource {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub total: Money,
    pub items_count: usize,
    pub status: OrderStatus,
}

impl From<Order> for CheckoutResource {
    fn from(order: Order) -> Self {
        let items_count = order.items.len();
        Self {
            order_id: order.id,
            user_id: order.user_id,
            total: order.total,
            items_count,
            status: order.status,
        }
    }
}

engine::respond!(CheckoutResource);

// ── CONTROLADOR HTTP (Solo invoca CheckoutService inyectado vía State) ────

pub fn routes() -> Router<AppContext> {
    Router::new().route("/", post(execute_checkout))
}

#[tracing::instrument(skip_all)]
async fn execute_checkout(
    ctx: Context,
    State(checkout): State<CheckoutService>,
    ValidatedBody(req): ValidatedBody<CheckoutRequest>,
) -> AppResult<Status<CheckoutResource>> {
    let items = req
        .items
        .into_iter()
        .map(|item| OrderItem::new(item.name, item.quantity, item.unit_price))
        .collect::<AppResult<Vec<_>>>()?;

    checkout
        .execute(
            &ctx,
            PlaceCheckout { user_id: req.user_id, idempotency_key: req.idempotency_key, items },
        )
        .await
        .map(CheckoutResource::from)
        .status(StatusCode::CREATED)
}
