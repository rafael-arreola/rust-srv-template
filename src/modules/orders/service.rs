use std::sync::Arc;

use super::dto::{CreateOrderInput, OrderOutput};
use super::entity::{Order, OrderItem};
use super::repository::OrderRepository;
use crate::core::context::Context;
use crate::core::error::{AppError, AppResult};

#[derive(Clone)]
pub struct OrderService {
    repo: Arc<OrderRepository>,
}

impl OrderService {
    pub fn new(repo: Arc<OrderRepository>) -> Self {
        Self { repo }
    }

    #[tracing::instrument(skip_all, fields(user_id = %input.user_id))]
    pub async fn create_order(
        &self,
        _ctx: &Context,
        input: CreateOrderInput,
    ) -> AppResult<OrderOutput> {
        let user_name = input.user_name.unwrap_or_else(|| "Customer".to_string());

        let items: Vec<OrderItem> = input
            .items
            .into_iter()
            .map(|item| OrderItem::new(item.name, item.quantity, item.unit_price))
            .collect();

        let order = Order::new(input.user_id, user_name, items);

        let created = self.repo.create(&order).await?;
        tracing::info!(order_id = %created.id, "Order created successfully");
        Ok(created.into())
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn get_order(&self, _ctx: &Context, id: &str) -> AppResult<OrderOutput> {
        let order =
            self.repo.find_by_id(id).await?.ok_or_else(|| AppError::not_found("Order", id))?;

        Ok(order.into())
    }
}
