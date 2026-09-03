pub mod controller;
pub mod dto;
pub mod model;
pub mod repository;
pub mod service;

use axum::Router;
use mongodb::Database;
use std::sync::Arc;

use self::repository::OrderRepository;
use self::service::OrderService;
use crate::core::error::AppResult;
use crate::modules::users::service::UserService;

#[derive(Clone)]
pub struct OrdersModule {
    pub service: Arc<OrderService>,
}

impl OrdersModule {
    pub async fn new(db: &Database, user_service: Arc<UserService>) -> AppResult<Self> {
        let repository = Arc::new(OrderRepository::new(db).await?);
        let service = Arc::new(OrderService::new(repository, user_service));
        Ok(Self { service })
    }

    pub fn router(&self) -> Router {
        controller::routes(self.service.clone())
    }
}
