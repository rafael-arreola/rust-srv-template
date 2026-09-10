use std::sync::Arc;

use super::dto::{CreateUserInput, UpdateUserInput, UserOutput};
use super::entity::User;
use super::repository::UserRepository;
use crate::core::error::{AppError, AppResult};

#[derive(Clone)]
pub struct UserService {
    repo: Arc<UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<UserRepository>) -> Self {
        Self { repo }
    }

    #[tracing::instrument(skip_all, fields(email = %input.email))]
    pub async fn create_user(&self, input: CreateUserInput) -> AppResult<UserOutput> {
        let existing = self.repo.find_by_email(&input.email).await?;
        if existing.is_some() {
            return Err(AppError::duplicate("User", "email", input.email));
        }

        let user = User::new(input.name, input.email);
        let created = self.repo.create(&user).await?;
        tracing::info!(user_id = %created.id, "User created successfully");
        Ok(created.into())
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn get_user(&self, id: &str) -> AppResult<UserOutput> {
        let user =
            self.repo.find_by_id(id).await?.ok_or_else(|| AppError::not_found("User", id))?;

        Ok(user.into())
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_users(&self, page: u32, limit: u32) -> AppResult<(Vec<UserOutput>, u64)> {
        let skip = ((page.max(1) - 1) * limit) as u64;
        let users = self.repo.find_all(skip, limit as i64).await?;
        let total = self.repo.count().await?;

        let outputs = users.into_iter().map(Into::into).collect();
        Ok((outputs, total))
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn update_user(&self, id: &str, input: UpdateUserInput) -> AppResult<UserOutput> {
        let mut user =
            self.repo.find_by_id(id).await?.ok_or_else(|| AppError::not_found("User", id))?;

        if input.email != user.email {
            let existing = self.repo.find_by_email(&input.email).await?;
            if existing.is_some() {
                return Err(AppError::duplicate("User", "email", input.email));
            }
        }

        user.update_profile(input.name, input.email);

        self.repo.update(id, &user).await?;
        tracing::info!(user_id = %id, "User updated");
        Ok(user.into())
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn delete_user(&self, id: &str) -> AppResult<()> {
        let deleted = self.repo.delete(id).await?;
        if !deleted {
            return Err(AppError::not_found("User", id));
        }
        tracing::info!(user_id = %id, "User soft-deleted");
        Ok(())
    }
}
