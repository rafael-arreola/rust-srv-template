//! # orders
//!
//! **Intent:** record a customer's purchase and its amount as a fact.
//! **Aggregate:** `Order` + `OrderItem`. Lifecycle: creation only — `OrderStatus` carries the
//!   state and no transitions are modelled yet.
//! **Invariants:** one order per `(user_id, idempotency_key)`, arbitrated by a unique index: a
//!   replay of the same purchase returns the recorded order, the same key with different items
//!   is a 409. Every amount is `Money` (integer minor units + currency) with checked
//!   arithmetic; all items share one currency; `total` and each `line_total` are derived once in
//!   the constructors and then recorded; `user_id` references `users` by id (an opaque `String` in
//!   the entity, stored as `ObjectId`) and the order stores nothing else about the user.
//! **Exposes to other modules:** `contract::OrderPlacement`, the only write door; there is no
//!   public creation route, so orders are written exclusively by the orchestrator that verified
//!   the user.
//! **Does not:** verify that the user exists (`checkout` does), group shipments, delete orders.

pub mod contract;
mod controller;
mod document;
pub mod dto;
mod entity;
mod repository;
pub mod service;

use axum::Router;
use std::sync::Arc;

use self::contract::OrderPlacement;
use self::repository::OrderRepository;
use self::service::OrderService;
use crate::core::db::Store;
use crate::core::error::AppResult;

#[derive(Clone)]
pub struct OrdersModule {
    pub service: Arc<OrderService>,
}

impl OrdersModule {
    pub async fn new(store: &Store) -> AppResult<Self> {
        let repository = Arc::new(OrderRepository::new(store).await?);
        let service = Arc::new(OrderService::new(repository));
        Ok(Self { service })
    }

    pub fn router(&self) -> Router {
        controller::routes(self.service.clone())
    }

    /// Puerta de escritura que el composition root entrega al orquestador.
    pub fn placement(&self) -> OrderPlacement {
        OrderPlacement::new(self.service.clone())
    }
}
