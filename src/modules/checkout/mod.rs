//! # checkout
//!
//! **Intent:** orchestrate a purchase inside one HTTP request — verify the user, then
//!   create the order. Composite slice: no persistence of its own.
//! **Owns:** only the `CheckoutInput`/`CheckoutOutput` contract.
//! **Invariants:** an order is created only for an existing user; it reaches `users` and
//!   `orders` through their public services and output DTOs, never their internals.
//! **Does not:** define user or order rules — those stay in their slices.

mod controller;
pub mod dto;
pub mod workflow;

use axum::Router;
use std::sync::Arc;

use self::workflow::CheckoutWorkflow;
use crate::modules::orders::service::OrderService;
use crate::modules::users::service::UserService;

/// Módulo Compuesto / Orquestador (Composite Slice) para el flujo de checkout.
/// Encapsula la orquestación entre Users y Orders sin persistencia directa propia.
#[derive(Clone)]
pub struct CheckoutModule {
    pub workflow: Arc<CheckoutWorkflow>,
}

impl CheckoutModule {
    pub fn new(user_service: Arc<UserService>, order_service: Arc<OrderService>) -> Self {
        let workflow = Arc::new(CheckoutWorkflow::new(user_service, order_service));
        Self { workflow }
    }

    pub fn router(&self) -> Router {
        controller::routes(self.workflow.clone())
    }
}
