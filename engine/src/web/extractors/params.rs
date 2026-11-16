use axum::{
    extract::{FromRequestParts, Path, Query},
    http::request::Parts,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use validator::Validate;

use super::validation_rejection;
use crate::error::AppError;

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct PaginationQuery {
    #[validate(range(min = 1, message = "Page must be greater than or equal to 1"))]
    pub page: Option<u32>,

    #[validate(range(min = 1, max = 100, message = "Limit must be between 1 and 100"))]
    pub limit: Option<u32>,
}

impl PaginationQuery {
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(20).clamp(1, 100)
    }

    pub fn skip(&self) -> u64 {
        u64::from(self.page().saturating_sub(1)).saturating_mul(u64::from(self.limit()))
    }
}

impl Default for PaginationQuery {
    fn default() -> Self {
        Self { page: Some(1), limit: Some(20) }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|e| AppError::invalid("query", e.to_string()))?;

        value.validate().map_err(|errs| validation_rejection("query", &errs))?;

        Ok(ValidatedQuery(value))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PathParam<T>(pub T);

impl<T, S> FromRequestParts<S> for PathParam<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(value) = Path::<T>::from_request_parts(parts, state).await.map_err(|e| {
            if e.status().is_server_error() {
                AppError::internal(format!("path extraction failed: {e}"))
            } else {
                AppError::invalid("path", e.body_text())
            }
        })?;

        Ok(PathParam(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::QueryResult;

    #[test]
    fn pagination_clamps_and_avoids_overflow() {
        let q = PaginationQuery { page: None, limit: None };
        assert_eq!((q.page(), q.limit(), q.skip()), (1, 20, 0));

        let huge = PaginationQuery { page: Some(u32::MAX), limit: Some(500) };
        assert_eq!(huge.limit(), 100);
        assert_eq!(huge.skip(), u64::from(u32::MAX - 1) * 100);

        let page = QueryResult::new(vec![1, 2], 21);
        assert_eq!(serde_json::to_string(&page).expect("json"), r#"{"items":[1,2],"total":21}"#);
    }
}
