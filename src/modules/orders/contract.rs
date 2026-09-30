use std::sync::Arc;

use super::dto::{CreateOrderInput, OrderOutput};
use super::service::OrderService;
use crate::core::context::Context;
use crate::core::error::AppResult;

/// Única puerta de escritura de `orders` para otros módulos.
///
/// `OrderService::create_order` es privado al slice y `orders` no expone ninguna ruta de
/// creación, así que este contrato es el único camino para escribir una orden. Quién lo recibe lo
/// decide `modules/mod.rs`, que lo entrega solo a `checkout`, el orquestador que verifica antes
/// que el usuario exista.
#[derive(Clone)]
pub struct OrderPlacement {
    service: Arc<OrderService>,
}

impl OrderPlacement {
    pub(super) fn new(service: Arc<OrderService>) -> Self {
        Self { service }
    }

    pub async fn place(&self, ctx: &Context, input: CreateOrderInput) -> AppResult<OrderOutput> {
        self.service.create_order(ctx, input).await
    }
}
