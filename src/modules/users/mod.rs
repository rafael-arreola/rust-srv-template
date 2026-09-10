pub(super) mod controller;
pub(super) mod document;
pub mod dto;
pub(super) mod entity;
pub(super) mod repository;
pub mod service;

use axum::Router;
use mongodb::Database;
use std::sync::Arc;

use self::repository::UserRepository;
use self::service::UserService;
use crate::core::error::AppResult;

#[derive(Clone)]
pub struct UsersModule {
    pub service: Arc<UserService>,
}

impl UsersModule {
    pub async fn new(db: &Database) -> AppResult<Self> {
        let repository = Arc::new(UserRepository::new(db).await?);
        let service = Arc::new(UserService::new(repository));
        Ok(Self { service })
    }

    pub fn router(&self) -> Router {
        controller::routes(self.service.clone())
    }
}
