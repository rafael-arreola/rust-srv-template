//! # users
//!
//! **Intent:** own the identity record of a customer — name and unique email.
//! **Aggregate:** `User`. No lifecycle states; profile edits go through `update_profile`,
//!   the only path that bumps `updated_at`. Fields are private, so the type enforces it.
//! **Invariants:** email is normalized to lowercase and unique among active users (compound
//!   unique index with `deleted_at`); deletion is logical (`deleted_at`); an edit carries the
//!   `version` the client read and is rejected with 409 when it is no longer current (another
//!   write happened since, even minutes earlier); the create race is arbitrated by the
//!   unique index and also surfaces as 409 `ALREADY_EXISTS`, with a generic public message.
//! **Exposes to other modules:** only `contract::UserLookup` (read-only).
//! **Does not:** authenticate (`core::auth`), know about orders or checkout.

pub mod contract;
mod controller;
mod document;
pub mod dto;
mod entity;
mod repository;
pub mod service;

use axum::Router;
use std::sync::Arc;

use self::contract::UserLookup;
use self::repository::UserRepository;
use self::service::UserService;
use crate::core::db::Store;
use crate::core::error::AppResult;

#[derive(Clone)]
pub struct UsersModule {
    pub service: Arc<UserService>,
}

impl UsersModule {
    pub async fn new(store: &Store) -> AppResult<Self> {
        let repository = Arc::new(UserRepository::new(store).await?);
        let service = Arc::new(UserService::new(repository));
        Ok(Self { service })
    }

    pub fn router(&self) -> Router {
        controller::routes(self.service.clone())
    }

    /// Contrato de lectura que el composition root entrega a los orquestadores.
    pub fn lookup(&self) -> UserLookup {
        UserLookup::new(self.service.clone())
    }
}
