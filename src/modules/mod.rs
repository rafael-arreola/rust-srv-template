pub mod health;
pub mod orders;
pub mod router;
pub mod users;

use axum::Router;

use crate::app::AppContext;
use crate::core::error::AppResult;

pub use self::orders::OrdersModule;
pub use self::router::api_v1_router;
pub use self::users::UsersModule;

/// Contenedor de inicialización y ciclo de vida de los módulos de dominio.
/// Resuelve el grafo topológico de dependencias de negocio sin mezclar lógica de enrutamiento HTTP.
pub struct Modules {
    pub users: UsersModule,
    pub orders: OrdersModule,
}

impl Modules {
    pub async fn init(ctx: &AppContext) -> AppResult<Self> {
        let users = UsersModule::new(&ctx.db).await?;
        let orders = OrdersModule::new(&ctx.db, users.service.clone()).await?;

        Ok(Self { users, orders })
    }
}

/// Punto de entrada para inicializar los módulos y construir el router de la API.
pub async fn build_api_router(ctx: &AppContext) -> AppResult<Router> {
    let modules = Modules::init(ctx).await?;
    Ok(router::api_v1_router(&modules))
}
