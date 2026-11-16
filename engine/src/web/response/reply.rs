use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use super::envelope::{ApiResponse, IntoStatus};
use crate::db::QueryResult;
use crate::error::AppResult;

/// Define cómo un tipo de dominio o recurso entra en `data` dentro del sobre uniforme de `engine`.
pub trait Respond: Sized {
    type Output: Serialize;
    fn try_into_output(self) -> AppResult<Self::Output>;

    #[inline]
    fn into_response_with(self, status: StatusCode) -> Response {
        match self.try_into_output() {
            Ok(output) => ApiResponse::with_status(status, output).into_response(),
            Err(err) => err.into_response(),
        }
    }
}

impl Respond for () {
    type Output = ();

    #[inline]
    fn try_into_output(self) -> AppResult<Self::Output> {
        Ok(())
    }

    #[inline]
    fn into_response_with(self, status: StatusCode) -> Response {
        ApiResponse::empty(status).into_response()
    }
}

impl<T: Respond> Respond for Vec<T> {
    type Output = Vec<T::Output>;

    #[inline]
    fn try_into_output(self) -> AppResult<Self::Output> {
        self.into_iter().map(T::try_into_output).collect()
    }
}

impl<T: Respond> Respond for Option<T> {
    type Output = Option<T::Output>;

    #[inline]
    fn try_into_output(self) -> AppResult<Self::Output> {
        self.map(T::try_into_output).transpose()
    }
}

impl<T: Respond> Respond for QueryResult<T> {
    type Output = QueryResult<T::Output>;

    #[inline]
    fn try_into_output(self) -> AppResult<Self::Output> {
        let items =
            self.items.into_iter().map(T::try_into_output).collect::<AppResult<Vec<_>>>()?;
        Ok(QueryResult::new(items, self.total))
    }
}

impl<T: Respond> IntoResponse for QueryResult<T> {
    #[inline]
    fn into_response(self) -> Response {
        self.into_response_with(StatusCode::OK)
    }
}

/// Conecta un modelo o recurso con el sobre de respuesta de `engine` en una sola línea:
/// - `engine::respond!(Product);` -> serializa `Product` directamente en `data`.
/// - `engine::respond!(User => UserResource);` -> convierte `User` vía `From<User>` automáticamente.
/// - `engine::respond!(try House => HouseResource);` -> convierte `House` vía `TryFrom<House, Error = AppError>`.
#[macro_export]
macro_rules! respond {
    ($ty:ty) => {
        impl $crate::web::Respond for $ty {
            type Output = $ty;
            #[inline]
            fn try_into_output(self) -> $crate::AppResult<Self::Output> {
                Ok(self)
            }
        }

        impl $crate::web::IntoResponse for $ty {
            #[inline]
            fn into_response(self) -> $crate::web::Response {
                <$ty as $crate::web::Respond>::into_response_with(self, $crate::web::StatusCode::OK)
            }
        }
    };
    ($entity:ty => $dto:ty) => {
        impl $crate::web::Respond for $entity {
            type Output = $dto;
            #[inline]
            fn try_into_output(self) -> $crate::AppResult<Self::Output> {
                Ok(<$dto as ::core::convert::From<$entity>>::from(self))
            }
        }

        impl $crate::web::IntoResponse for $entity {
            #[inline]
            fn into_response(self) -> $crate::web::Response {
                <$entity as $crate::web::Respond>::into_response_with(
                    self,
                    $crate::web::StatusCode::OK,
                )
            }
        }
    };
    (try $entity:ty => $dto:ty) => {
        impl $crate::web::Respond for $entity {
            type Output = $dto;
            #[inline]
            fn try_into_output(self) -> $crate::AppResult<Self::Output> {
                <$dto as ::core::convert::TryFrom<$entity>>::try_from(self)
            }
        }

        impl $crate::web::IntoResponse for $entity {
            #[inline]
            fn into_response(self) -> $crate::web::Response {
                <$entity as $crate::web::Respond>::into_response_with(
                    self,
                    $crate::web::StatusCode::OK,
                )
            }
        }
    };
}

/// Envoltorio universal de código HTTP sobre el sobre uniforme de `engine`.
///
/// Permite responder cualquier código mediante primitivas tipadas de Axum (`StatusCode::CREATED`,
/// `StatusCode::ACCEPTED`) o su número directo (`201`, `202`, `204`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status<T = ()>(pub StatusCode, pub T);

impl<T> Status<T> {
    #[inline]
    pub fn new(status: impl IntoStatus, body: T) -> Self {
        Self(status.into_status(), body)
    }
}

impl Status<()> {
    #[inline]
    pub fn code(status: impl IntoStatus) -> Self {
        Self(status.into_status(), ())
    }
}

impl<T: Respond> IntoResponse for Status<T> {
    #[inline]
    fn into_response(self) -> Response {
        self.1.into_response_with(self.0)
    }
}

/// Extensión ergonómica para finalizar handlers en 1 sola línea:
/// - `service.call(...).await.status(StatusCode::ACCEPTED)` o `.status(202)`
/// - Atajos básicos por defecto: `.created()` (`201`) y `.no_content()` (`204`).
pub trait ReplyExt<T> {
    fn status(self, code: impl IntoStatus) -> AppResult<Status<T>>
    where
        T: Respond;

    fn created(self) -> AppResult<Status<T>>
    where
        T: Respond;

    fn no_content(self) -> AppResult<Status>
    where
        T: Into<()>;
}

impl<T> ReplyExt<T> for AppResult<T> {
    #[inline]
    fn status(self, code: impl IntoStatus) -> AppResult<Status<T>>
    where
        T: Respond,
    {
        let status = code.into_status();
        self.map(|body| Status(status, body))
    }

    #[inline]
    fn created(self) -> AppResult<Status<T>>
    where
        T: Respond,
    {
        self.status(StatusCode::CREATED)
    }

    #[inline]
    fn no_content(self) -> AppResult<Status>
    where
        T: Into<()>,
    {
        self.map(|_| Status::code(StatusCode::NO_CONTENT))
    }
}
