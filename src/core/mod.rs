pub mod db;
pub mod http;
#[macro_use]
pub mod macros;
pub mod net;
pub mod telemetry;

// Backward-compatible module aliases
pub use http::error;
pub use http::extractors;
pub use http::middleware;
pub use http::pagination;
pub use http::response;
pub use http::server;
pub use net::client as http_client;
pub use telemetry::tracer;

// Direct type re-exports
pub use db::{MongoProvider, RedisProvider};
pub use http::{
    ApiResponse, AppError, AppResult, ContextExt, ErrorKind, ErrorSeverity, GenericApiResponse,
    GenericPagination, Paginated, PaginatedResponse, PaginationQuery, ServerLauncher,
    ValidatedBody,
};
pub use net::{client_with_timeout, instrumented_client};
pub use telemetry::{TracerGuard, init_tracing};
