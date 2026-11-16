pub mod auth;
pub mod extractors;
pub mod format;
pub mod layers;
#[macro_use]
pub mod response;
pub mod server;

pub use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, FromRef, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};

pub use self::auth::{
    Auth, AuthPrincipal, AuthProvider, Policy, RequirePermissionExt, RouterAuthExt, TaskToken,
    TokenAuthProvider,
};
#[cfg(feature = "multipart")]
pub use self::extractors::{FieldData, TryFromMultipart, ValidatedMultipart};
pub use self::extractors::{
    PaginationQuery, PathParam, ValidatedBody, ValidatedBodyWithRaw, ValidatedQuery,
};
pub use self::format::{CONTENT_TYPE_MSGPACK, is_msgpack_content_type, wants_msgpack_response};
pub use self::layers::{HeaderContextExt, require_header_context};
pub use self::response::{ApiResponse, FileResponse, ReplyExt, Respond, Status};
pub use self::server::{
    ServerLauncher, health_router, is_draining, method_not_allowed, not_found, start_draining,
};
pub use crate::db::QueryResult;

/// Conecta los servicios del contenedor raíz (`AppContext`) con Axum `State<T>` para inyectarlos
/// individualmente en los controladores HTTP sin escribir `impl FromRef` repetitivos.
#[macro_export]
macro_rules! provide {
    ($ctx:ty => $($field:ident : $dep:ty),+ $(,)?) => {
        $(
            impl $crate::web::FromRef<$ctx> for $dep {
                #[inline]
                fn from_ref(state: &$ctx) -> Self {
                    state.$field.clone()
                }
            }
        )+
    };
}
