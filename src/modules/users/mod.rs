//! # users
//!
//! **Intent:** own the identity record of a customer — name and unique email.
//! **Aggregate:** `User`. No lifecycle states; profile edits go through `update_profile`,
//!   which is the only path that bumps `updated_at`.
//! **Invariants:** email is unique (unique index on the collection).
//! **Does not:** authenticate (`core::auth`), know about orders or checkout.

mod controller;
mod document;
pub mod dto;
mod entity;
mod repository;
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
