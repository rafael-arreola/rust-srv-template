use axum::Router;

use super::Modules;
use super::checkout::CheckoutModule;
use super::orders::OrdersModule;
use super::users::UsersModule;

/// Construye el árbol de rutas HTTP montando los sub-routers de cada módulo de dominio bajo `/api/v1`.
pub fn api_v1_router(modules: &Modules) -> Router {
    Router::new()
        .nest("/users", users_routes(&modules.users))
        .nest("/orders", orders_routes(&modules.orders))
        .nest("/checkout", checkout_routes(&modules.checkout))
}

/// Rutas del dominio de usuarios (Leaf Slice).
pub fn users_routes(users: &UsersModule) -> Router {
    users.router()
}

/// Rutas del dominio de órdenes (Leaf Slice).
pub fn orders_routes(orders: &OrdersModule) -> Router {
    orders.router()
}

/// Rutas del flujo de checkout (Composite Slice / Orquestador).
pub fn checkout_routes(checkout: &CheckoutModule) -> Router {
    checkout.router()
}
