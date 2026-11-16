use engine::web::{PathParam, Router, State, get};
use engine::{AppResult, Context, UtcDateTime};
use serde::{Deserialize, Serialize};

use crate::bootstrap::AppContext;
use crate::models::{Money, Order, OrderId, OrderItem, OrderStatus, UserId};
use crate::services::OrderService;

// ── JSON RESOURCES ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItemResource {
    pub name: String,
    pub quantity: u32,
    pub unit_price: Money,
    pub total: Money,
}

impl From<OrderItem> for OrderItemResource {
    fn from(item: OrderItem) -> Self {
        Self {
            name: item.name,
            quantity: item.quantity,
            unit_price: item.unit_price,
            total: item.line_total,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResource {
    pub id: OrderId,
    pub user_id: UserId,
    pub items: Vec<OrderItemResource>,
    pub total: Money,
    pub status: OrderStatus,
    pub created_at: UtcDateTime,
}

impl From<Order> for OrderResource {
    fn from(order: Order) -> Self {
        Self {
            id: order.id,
            user_id: order.user_id,
            items: order.items.into_iter().map(OrderItemResource::from).collect(),
            total: order.total,
            status: order.status,
            created_at: order.created_at,
        }
    }
}

engine::respond!(Order => OrderResource);

// ── CONTROLADOR HTTP (Solo invoca OrderService inyectado vía State) ───────

pub fn routes() -> Router<AppContext> {
    Router::new().route("/{id}", get(get_order))
}

#[tracing::instrument(skip_all)]
async fn get_order(
    ctx: Context,
    State(orders): State<OrderService>,
    PathParam(id): PathParam<OrderId>,
) -> AppResult<Order> {
    orders.get_by_id(&ctx, &id).await
}
