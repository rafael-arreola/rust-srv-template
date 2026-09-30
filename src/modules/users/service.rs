use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Utc;

use super::dto::{CreateUserInput, PaginatedUsers, UpdateUserInput, UserOutput};
use super::entity::User;
use super::repository::{UpdateOutcome, UserRepository};
use crate::core::context::Context;
use crate::core::error::{AppError, AppResult};
use crate::core::pagination::{Paginated, PaginationQuery};

#[derive(Clone)]
pub struct UserService {
    repo: Arc<UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<UserRepository>) -> Self {
        Self { repo }
    }

    /// La comprobación previa del email da un mensaje preciso; la carrera entre dos altas
    /// simultáneas la arbitra el índice único, cuya violación `core::db` traduce también a 409
    /// `ALREADY_EXISTS` (con un mensaje público genérico).
    #[tracing::instrument(skip_all)]
    pub async fn create_user(
        &self,
        _ctx: &Context,
        input: CreateUserInput,
    ) -> AppResult<UserOutput> {
        let user = User::new(input.name, input.email, Utc::now())?;

        if self.repo.find_by_email(user.email()).await?.is_some() {
            return Err(AppError::already_exists("User", "email already in use"));
        }

        let created = self.repo.create(&user).await?;
        tracing::info!(user_id = %created.id(), "User created successfully");
        Ok(created.into())
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn get_user(&self, _ctx: &Context, id: &str) -> AppResult<UserOutput> {
        let user =
            self.repo.find_by_id(id).await?.ok_or_else(|| AppError::not_found("User", id))?;

        Ok(user.into())
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_users(
        &self,
        _ctx: &Context,
        query: &PaginationQuery,
    ) -> AppResult<PaginatedUsers> {
        let result = self.repo.list(query.skip(), query.limit()).await?.map(Into::into);
        Ok(Paginated::from_query(result, query))
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn update_user(
        &self,
        _ctx: &Context,
        id: &str,
        input: UpdateUserInput,
    ) -> AppResult<UserOutput> {
        let mut user =
            self.repo.find_by_id(id).await?.ok_or_else(|| AppError::not_found("User", id))?;

        if user.version() != input.version {
            return Err(concurrent_modification(id, input.version));
        }

        let previous_email = user.email().to_string();
        user.update_profile(input.name, input.email, Utc::now())?;

        if user.email() != previous_email && self.repo.find_by_email(user.email()).await?.is_some()
        {
            return Err(AppError::already_exists("User", "email already in use"));
        }

        match self.repo.update(&user).await? {
            UpdateOutcome::Updated => {}
            UpdateOutcome::NotFound => return Err(AppError::not_found("User", id)),
            UpdateOutcome::Conflict => return Err(concurrent_modification(id, input.version)),
        }

        tracing::info!(user_id = %id, "User updated");
        let updated =
            self.repo.find_by_id(id).await?.ok_or_else(|| AppError::not_found("User", id))?;
        Ok(updated.into())
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn delete_user(&self, _ctx: &Context, id: &str) -> AppResult<()> {
        let deleted = self.repo.soft_delete(id, Utc::now()).await?;
        if !deleted {
            return Err(AppError::not_found("User", id));
        }
        tracing::info!(user_id = %id, "User soft-deleted");
        Ok(())
    }
}

/// La versión que el cliente leyó ya no es la vigente: otra petición escribió antes.
fn concurrent_modification(id: &str, version: u32) -> AppError {
    AppError::censored(
        StatusCode::CONFLICT,
        "CONCURRENT_MODIFICATION",
        "The user was modified by another request. Reload it and retry.",
        format!("optimistic lock failed for user {id} at version {version}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::db::Store;

    async fn service(store: &Store) -> UserService {
        UserService::new(Arc::new(UserRepository::new(store).await.expect("repository")))
    }

    fn create(name: &str, email: &str) -> CreateUserInput {
        CreateUserInput { name: name.into(), email: email.into() }
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn create_normalizes_email_and_rejects_a_duplicate() {
        let store = Store::connect_for_tests().await;
        let users = service(&store).await;
        let ctx = Context::new();

        let created =
            users.create_user(&ctx, create("Ana", "Ana@Example.com")).await.expect("created");
        assert_eq!((created.email.as_str(), created.version), ("ana@example.com", 0));
        assert_eq!(users.get_user(&ctx, &created.id).await.expect("found").id, created.id);

        let err = users
            .create_user(&ctx, create("Otra", "ana@example.COM"))
            .await
            .expect_err("duplicate");
        assert_eq!((err.code(), err.status_code()), ("ALREADY_EXISTS", StatusCode::CONFLICT));

        store.drop_for_tests().await;
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn an_edit_with_a_stale_version_is_a_conflict() {
        let store = Store::connect_for_tests().await;
        let users = service(&store).await;
        let ctx = Context::new();
        let created =
            users.create_user(&ctx, create("Ana", "ana@example.com")).await.expect("created");

        let edit = |name: &str, version| UpdateUserInput {
            name: name.into(),
            email: "ana@example.com".into(),
            version,
        };
        let updated =
            users.update_user(&ctx, &created.id, edit("Ana María", 0)).await.expect("updated");
        assert_eq!((updated.name.as_str(), updated.version), ("Ana María", 1));

        let err = users
            .update_user(&ctx, &created.id, edit("Pisada", 0))
            .await
            .expect_err("stale version");
        assert_eq!(
            (err.code(), err.status_code()),
            ("CONCURRENT_MODIFICATION", StatusCode::CONFLICT)
        );

        store.drop_for_tests().await;
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn a_deleted_user_is_gone_and_its_email_is_free_again() {
        let store = Store::connect_for_tests().await;
        let users = service(&store).await;
        let ctx = Context::new();
        let created =
            users.create_user(&ctx, create("Ana", "ana@example.com")).await.expect("created");

        users.delete_user(&ctx, &created.id).await.expect("deleted");
        assert_eq!(users.get_user(&ctx, &created.id).await.expect_err("gone").code(), "NOT_FOUND");
        assert_eq!(
            users.delete_user(&ctx, &created.id).await.expect_err("twice").code(),
            "NOT_FOUND"
        );
        users.create_user(&ctx, create("Ana", "ana@example.com")).await.expect("email reusable");

        store.drop_for_tests().await;
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn an_id_that_is_not_an_object_id_is_not_found() {
        let store = Store::connect_for_tests().await;
        let users = service(&store).await;

        let err = users.get_user(&Context::new(), "not-an-id").await.expect_err("malformed id");
        assert_eq!((err.code(), err.status_code()), ("NOT_FOUND", StatusCode::NOT_FOUND));

        store.drop_for_tests().await;
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn listing_pages_active_users_newest_first() {
        let store = Store::connect_for_tests().await;
        let users = service(&store).await;
        let ctx = Context::new();
        for n in 0..3 {
            users
                .create_user(&ctx, create("User", &format!("u{n}@example.com")))
                .await
                .expect("created");
        }

        let page = users
            .list_users(&ctx, &PaginationQuery { page: Some(1), limit: Some(2) })
            .await
            .expect("listed");
        assert_eq!((page.total, page.total_pages, page.data.len()), (3, 2, 2));
        assert_eq!(page.data.first().map(|u| u.email.as_str()), Some("u2@example.com"));

        store.drop_for_tests().await;
    }
}
