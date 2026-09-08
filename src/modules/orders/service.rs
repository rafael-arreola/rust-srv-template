use chrono::Utc;
use std::sync::Arc;

use super::dto::{CreateOrderInput, OrderOutput};
use super::model::{OrderItemModel, OrderModel};
use super::repository::OrderRepository;
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
    pub async fn create_order(&self, input: CreateOrderInput) -> AppResult<OrderOutput> {
        let user_name = input.user_name.unwrap_or_else(|| "Customer".to_string());

        let mut total_amount = 0.0;
        let mut item_models = Vec::with_capacity(input.items.len());

        for item in input.items {
            total_amount += item.quantity as f64 * item.unit_price;
            item_models.push(OrderItemModel {
                name: item.name,
                quantity: item.quantity,
                unit_price: item.unit_price,
            });
        }

        let order_model = OrderModel {
            id: None,
            user_id: input.user_id,
            user_name,
            items: item_models,
            total_amount,
            status: "CREATED".to_string(),
            created_at: Utc::now(),
            deleted_at: None,
        };

        let created = self.repo.create(order_model).await?;
        tracing::info!(order_id = ?created.id, "Order created successfully");
        Ok(created.to_output())
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn get_order(&self, id: &str) -> AppResult<OrderOutput> {
        let order =
            self.repo.find_by_id(id).await?.ok_or_else(|| AppError::not_found("Order", id))?;

        Ok(order.to_output())
    }
}
