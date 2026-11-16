use engine::{AppError, AppResult, Context, QueryResult, StatusCode, UtcDateTime};

use crate::models::{User, UserId};
use crate::stores::{UpdateOutcome, UserFilter, UserStore};

#[derive(Clone)]
pub struct UserService {
    store: UserStore,
}

impl UserService {
    pub fn new(store: UserStore) -> Self {
        Self { store }
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn find_active(&self, _ctx: &Context, id: &UserId) -> AppResult<User> {
        self.store.find_active(id).await?.ok_or_else(|| AppError::not_found("User", id.as_str()))
    }

    #[tracing::instrument(skip_all)]
    pub async fn create(&self, _ctx: &Context, name: String, email: String) -> AppResult<User> {
        let user = User::new(name, email, UtcDateTime::now())?;

        if self.store.email_exists(&user.email).await? {
            return Err(AppError::already_exists("User", "email already in use"));
        }

        self.store.insert(&user).await?;
        tracing::info!(user_id = %user.id, "User created successfully");
        Ok(user)
    }

    #[tracing::instrument(skip_all)]
    pub async fn list(
        &self,
        _ctx: &Context,
        search: Option<String>,
        skip: u64,
        limit: u32,
    ) -> AppResult<QueryResult<User>> {
        self.store.list_active(&UserFilter { search }, skip, limit).await
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn update(
        &self,
        _ctx: &Context,
        id: &UserId,
        name: String,
        email: String,
        version: u32,
    ) -> AppResult<User> {
        let mut user = self
            .store
            .find_active(id)
            .await?
            .ok_or_else(|| AppError::not_found("User", id.as_str()))?;

        if user.version != version {
            return Err(concurrent_modification(id, version));
        }

        let previous_email = user.email.clone();
        user.update_profile(name, email, UtcDateTime::now())?;

        if user.email != previous_email && self.store.email_exists(&user.email).await? {
            return Err(AppError::already_exists("User", "email already in use"));
        }

        match self.store.update_versioned(&user).await? {
            UpdateOutcome::Updated(updated) => {
                tracing::info!(user_id = %id, "User updated");
                Ok(updated)
            }
            UpdateOutcome::NotFound => Err(AppError::not_found("User", id.as_str())),
            UpdateOutcome::Conflict => Err(concurrent_modification(id, version)),
        }
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn delete(&self, _ctx: &Context, id: &UserId) -> AppResult<()> {
        if !self.store.soft_delete(id, UtcDateTime::now()).await? {
            return Err(AppError::not_found("User", id.as_str()));
        }
        tracing::info!(user_id = %id, "User soft-deleted");
        Ok(())
    }
}

fn concurrent_modification(id: &UserId, version: u32) -> AppError {
    AppError::censored(
        StatusCode::CONFLICT,
        "CONCURRENT_MODIFICATION",
        "The user was modified by another request. Reload it and retry.",
        format!("optimistic lock failed for user {id} at version {version}"),
    )
}

#[cfg(test)]
mod tests {
    use engine::{Mongo, MongoDb};

    use super::*;

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn full_lifecycle_crud_optimistic_lock_and_soft_delete() {
        let db = MongoDb::connect_for_tests().await;

        Mongo::scope(Mongo::DEFAULT, db.clone(), async {
            let store = UserStore::new().expect("user store");
            store.init_indexes().await.expect("indexes");
            let svc = UserService::new(store);
            let ctx = Context::new();

            let created =
                svc.create(&ctx, "Ana".into(), "Ana@Example.com".into()).await.expect("created");
            assert_eq!((created.email.as_str(), created.version), ("ana@example.com", 0));

            let updated = svc
                .update(&ctx, &created.id, "Ana M.".into(), "ana@example.com".into(), 0)
                .await
                .expect("updated");
            assert_eq!(updated.version, 1);

            let stale = svc
                .update(&ctx, &created.id, "Stale".into(), "ana@example.com".into(), 0)
                .await
                .expect_err("stale");
            assert_eq!(stale.code(), "CONCURRENT_MODIFICATION");

            svc.delete(&ctx, &created.id).await.expect("deleted");
            assert_eq!(
                svc.find_active(&ctx, &created.id).await.expect_err("gone").code(),
                "NOT_FOUND"
            );
        })
        .await;

        db.drop_for_tests().await;
    }
}
