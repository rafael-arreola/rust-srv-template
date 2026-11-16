use engine::{AppResult, Http};

use crate::clients::CarrierClient;
use crate::envs;
use crate::services::{CheckoutService, OrderService, UserService};
use crate::stores::{OrderStore, UserStore};

/// Contenedor raíz de servicios de negocio ensamblados por inyección de dependencias.
///
/// Expone exclusivamente `*Service` hacia la capa HTTP (nunca stores/repositorios ni clientes crudos).
#[derive(Clone)]
pub struct AppContext {
    pub users: UserService,
    pub orders: OrderService,
    pub checkout: CheckoutService,
}

engine::provide!(AppContext =>
    users: UserService,
    orders: OrderService,
    checkout: CheckoutService,
);

impl AppContext {
    pub async fn init() -> AppResult<Self> {
        let env = envs::get();

        let user_store = UserStore::new()?;
        let order_store = OrderStore::new()?;
        tokio::try_join!(user_store.init_indexes(), order_store.init_indexes())?;

        let carrier = match (&env.carrier_url, &env.carrier_api_key) {
            (Some(url), Some(key)) => {
                Some(CarrierClient::new(Http::get(Http::DEFAULT)?, url.clone(), key.clone())?)
            }
            _ => None,
        };

        let users = UserService::new(user_store);
        let orders = OrderService::new(order_store);
        let checkout = CheckoutService::new(users.clone(), orders.clone(), carrier);

        Ok(Self { users, orders, checkout })
    }
}
