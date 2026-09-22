pub mod auth;
pub mod context;
pub mod db;
pub mod http;
#[macro_use]
pub mod macros;
pub mod net;
pub mod telemetry;

// Rutas cortas canónicas para las familias de transporte HTTP y telemetría.
pub use http::error;
pub use http::extractors;
pub use http::pagination;
pub use http::response;
pub use http::server;
pub use telemetry::tracer;

// Direct type re-exports
pub use auth::{
    Actor, AuthPrincipal, AuthProvider, JwtJwksAuthProvider, JwtJwksClaims, TaskToken,
    auth_middleware, extract_bearer_token, require_any_permission, require_permission,
    task_auth_middleware,
};
pub use context::{Context, ContextRequestExt, ContextValue};
pub use db::{MongoProvider, QueryResult, RedisProvider, parse_object_id};
pub use http::{
    ApiResponse, AppError, AppResult, CONTENT_TYPE_MSGPACK, ContextExt, ErrorKind, ErrorSeverity,
    MessageOutput, MsgPack, Paginated, PaginationQuery, PathParam, ServerLauncher,
    TaskSuccessOutput, ValidatedBody, ValidatedQuery, is_msgpack_content_type,
    wants_msgpack_response,
};
pub use net::{APP_USER_AGENT, HttpClient};
pub use telemetry::{TracerGuard, init_tracing};
