use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, OnceLock, PoisonError, RwLock};

use crate::error::{AppError, AppResult};

type ScopedMap = HashMap<(TypeId, String), Arc<dyn Any + Send + Sync>>;

tokio::task_local! {
    static SCOPED: Arc<RwLock<ScopedMap>>;
}

/// Contenedor genérico de singletons nombrados estilo Facade (`"default"`, `"analytics"`, etc.).
///
/// - En producción opera sobre un `RwLock<HashMap>` estático de lectura inmediata.
/// - En pruebas soporta [`Registry::scope`], aislando instancias por tarea asíncrona para que
///   decenas de tests en paralelo usen `"default"` sin pisarse entre sí.
/// - Reutilizable para cualquier motor (`Mongo`, `Redis`, `Http`, `Postgres`, `SqlServer`...).
pub struct Registry<T: 'static> {
    kind: &'static str,
    global: OnceLock<RwLock<HashMap<String, T>>>,
}

impl<T: Clone + Send + Sync + 'static> Registry<T> {
    pub const fn new(kind: &'static str) -> Self {
        Self { kind, global: OnceLock::new() }
    }

    fn map(&self) -> &RwLock<HashMap<String, T>> {
        self.global.get_or_init(|| RwLock::new(HashMap::new()))
    }

    /// Registra o reemplaza globalmente la instancia bajo `instance`.
    pub fn register(&self, instance: impl Into<String>, value: T) {
        let mut guard = self.map().write().unwrap_or_else(PoisonError::into_inner);
        guard.insert(instance.into(), value);
    }

    /// Obtiene la instancia `instance` si existe (primero revisa el scope de tarea y luego el global).
    pub fn try_get(&self, instance: &str) -> Option<T> {
        if let Ok(Some(scoped)) = SCOPED.try_with(|map| {
            let guard = map.read().unwrap_or_else(PoisonError::into_inner);
            guard
                .get(&(TypeId::of::<T>(), instance.to_string()))
                .and_then(|any| any.downcast_ref::<T>().cloned())
        }) {
            return Some(scoped);
        }

        let guard = self.map().read().unwrap_or_else(PoisonError::into_inner);
        guard.get(instance).cloned()
    }

    /// Obtiene la instancia `instance` o falla con `500 INTERNAL_ERROR` indicando origen `archivo:línea`.
    #[track_caller]
    pub fn get(&self, instance: &str) -> AppResult<T> {
        self.try_get(instance).ok_or_else(|| {
            AppError::internal(format!(
                "{} instance '{instance}' has not been initialized",
                self.kind
            ))
        })
    }

    /// Aísla una instancia `instance` únicamente durante la ejecución del future `fut`.
    pub async fn scope<F: Future>(
        &self,
        instance: impl Into<String>,
        value: T,
        fut: F,
    ) -> F::Output {
        let mut next = SCOPED
            .try_with(|existing| existing.read().unwrap_or_else(PoisonError::into_inner).clone())
            .unwrap_or_default();

        next.insert((TypeId::of::<T>(), instance.into()), Arc::new(value));
        SCOPED.scope(Arc::new(RwLock::new(next)), fut).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SAMPLE: Registry<String> = Registry::new("Sample");

    #[tokio::test]
    async fn global_registration_and_parallel_task_scopes_do_not_collide() {
        assert!(SAMPLE.get("missing").is_err());

        SAMPLE.register("default", "prod_conn".to_string());
        assert_eq!(SAMPLE.get("default").expect("global").as_str(), "prod_conn");

        let (a, b) = tokio::join!(
            SAMPLE.scope("default", "test_db_a".to_string(), async {
                tokio::task::yield_now().await;
                SAMPLE.get("default").expect("scoped a")
            }),
            SAMPLE.scope("default", "test_db_b".to_string(), async {
                tokio::task::yield_now().await;
                SAMPLE.get("default").expect("scoped b")
            }),
        );

        assert_eq!(a, "test_db_a");
        assert_eq!(b, "test_db_b");
        assert_eq!(SAMPLE.get("default").expect("global intact"), "prod_conn");
    }
}
