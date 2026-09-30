//! # checkout
//!
//! **Intent:** orchestrate a purchase inside one HTTP request — verify the user, then
//!   record the order. Composite slice: no persistence of its own.
//! **Owns:** only the `CheckoutInput`/`CheckoutOutput` contract.
//! **Invariants:** an order is created only for an existing, active user. It holds the narrow
//!   contracts `users::contract::UserLookup` and `orders::contract::OrderPlacement`, never the
//!   full services nor their internals. The composition root (`modules/mod.rs`) hands
//!   `OrderPlacement` only to this slice; that is wiring discipline, not a compiler guarantee.
//!   A retry with the same `idempotency_key` returns the order already recorded.
//! **Accepted race:** the user check and the order write are two operations with no transaction.
//!   A user deleted between them still gets this one order; that window (milliseconds) is
//!   accepted because users are soft-deleted and orders keep only the opaque `user_id`. A slice
//!   that cannot accept it needs a transaction or a re-check inside the write, not a longer check.
//! **Does not:** define user or order rules — those stay in their slices; copy user data into
//!   its output.

mod controller;
pub mod dto;
pub mod workflow;

use axum::Router;
use std::sync::Arc;

use self::workflow::CheckoutWorkflow;
use crate::modules::orders::contract::OrderPlacement;
use crate::modules::users::contract::UserLookup;

/// Módulo Compuesto / Orquestador (Composite Slice) para el flujo de checkout.
#[derive(Clone)]
pub struct CheckoutModule {
    pub workflow: Arc<CheckoutWorkflow>,
}

impl CheckoutModule {
    pub fn new(users: UserLookup, orders: OrderPlacement) -> Self {
        let workflow = Arc::new(CheckoutWorkflow::new(users, orders));
        Self { workflow }
    }

    pub fn router(&self) -> Router {
        controller::routes(self.workflow.clone())
    }
}
