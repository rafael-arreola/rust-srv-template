pub(super) mod controller;
pub(super) mod document;
pub mod dto;
pub(super) mod entity;
pub(super) mod repository;
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
