pub mod checkout;
pub mod health;
pub mod orders;
pub mod users;

use axum::Router;

use self::checkout::CheckoutModule;
use self::orders::OrdersModule;
use self::users::UsersModule;
use crate::app::AppContext;
use crate::core::error::AppResult;

/// Contenedor de inicialización y ciclo de vida de los módulos de dominio.
/// Resuelve el grafo topológico de dependencias de negocio y monta sus rutas bajo `/api/v1`.
///
/// Alta de un slice: declarar `pub mod`, añadir el campo, construirlo en `init` (por estrato) y
/// anidarlo en `api_v1_router`. Es el único archivo de wiring de módulos.
pub struct Modules {
    users: UsersModule,
    orders: OrdersModule,
    checkout: CheckoutModule,
}

impl Modules {
    pub async fn init(ctx: &AppContext) -> AppResult<Self> {
        // Nivel 0: Leaf Slices (Módulos Base independientes). Reciben el `Store` neutral; solo su
        // `repository.rs` baja al driver.
        let users = UsersModule::new(&ctx.db).await?;
        let orders = OrdersModule::new(&ctx.db).await?;

        // Nivel 1: Composite Slices. El orquestador recibe los contratos estrechos de cada módulo
        // base, no sus services: `OrderPlacement` es la única puerta de escritura de órdenes y
        // solo `checkout` la posee.
        let checkout = CheckoutModule::new(users.lookup(), orders.placement());

        Ok(Self { users, orders, checkout })
    }

    /// Árbol de rutas de la API pública: cada slice declara rutas relativas y aquí se les asigna
    /// el nombre del recurso. El prefijo `/api/v1` lo adjunta `app.rs`.
    pub fn api_v1_router(&self) -> Router {
        Router::new()
            .nest("/users", self.users.router())
            .nest("/orders", self.orders.router())
            .nest("/checkout", self.checkout.router())
    }
}
