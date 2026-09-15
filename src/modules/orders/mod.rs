//! # orders
//!
//! **Intent:** record a customer's purchase and its amount as a fact.
//! **Aggregate:** `Order` + `OrderItem`. Lifecycle: creation only — `status` carries the
//!   state and no transitions are modelled yet.
//! **Invariants:** `total_amount` is derived from the items in `Order::new`; `user_id` is an
//!   opaque reference to `users` and never changes.
//! **Does not:** verify that the user exists (`checkout` does), group shipments.

mod controller;
mod document;
pub mod dto;
mod entity;
mod repository;
pub mod service;

use axum::Router;
use mongodb::Database;
use std::sync::Arc;

use self::repository::OrderRepository;
use self::service::OrderService;
use crate::core::error::AppResult;

#[derive(Clone)]
pub struct OrdersModule {
    pub service: Arc<OrderService>,
}

impl OrdersModule {
    pub async fn new(db: &Database) -> AppResult<Self> {
        let repository = Arc::new(OrderRepository::new(db).await?);
        let service = Arc::new(OrderService::new(repository));
        Ok(Self { service })
    }

    pub fn router(&self) -> Router {
        controller::routes(self.service.clone())
    }
}
