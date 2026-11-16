use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use axum::extract::FromRequestParts;
use axum::http::Extensions;
use axum::http::request::Parts;

use crate::error::{AppError, AppResult};

type Values = HashMap<TypeId, Arc<dyn Any + Send + Sync>>;

/// Bolsa tipada de valores de la operación en curso.
#[derive(Clone, Default)]
pub struct Context {
    values: Arc<RwLock<Values>>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_extensions(extensions: &mut Extensions) -> Self {
        if let Some(existing) = extensions.get::<Self>() {
            return existing.clone();
        }
        let fresh = Self::new();
        extensions.insert(fresh.clone());
        fresh
    }

    #[must_use]
    pub fn with<T: Any + Clone + Send + Sync + 'static>(self, value: T) -> Self {
        self.insert(value);
        self
    }

    pub fn insert<T: Any + Clone + Send + Sync + 'static>(&self, value: T) -> Option<T> {
        self.write()
            .insert(TypeId::of::<T>(), Arc::new(value))
            .and_then(|previous| previous.downcast_ref::<T>().cloned())
    }

    pub fn get<T: Any + Clone + Send + Sync + 'static>(&self) -> Option<T> {
        self.read().get(&TypeId::of::<T>()).and_then(|value| value.downcast_ref::<T>()).cloned()
    }

    pub fn require<T: Any + Clone + Send + Sync + 'static>(&self) -> AppResult<T> {
        self.get::<T>().ok_or_else(|| {
            AppError::internal(format!(
                "operation context has no value of type `{}`",
                type_name::<T>()
            ))
        })
    }

    fn read(&self) -> RwLockReadGuard<'_, Values> {
        self.values.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Values> {
        self.values.write().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for Context {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Context").field("entries", &self.read().len()).finish()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Context {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::from_extensions(&mut parts.extensions))
    }
}

/// Proyección del actor autenticado que viaja en el [`Context`] sin exponer el token ni permisos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor {
    pub provider: &'static str,
    pub client_id: String,
    pub email: Option<String>,
}

/// Credencial Bearer verificada y conservada en [`Context`] en rutas montadas con
/// `with_relay_auth` para llamadas delegadas desde servicios o clientes hacia APIs internas.
#[derive(Clone, PartialEq, Eq)]
pub struct DelegatedCredential(String);

impl DelegatedCredential {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DelegatedCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DelegatedCredential(<redacted>)")
    }
}
