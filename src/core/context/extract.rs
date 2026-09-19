//! Puente entre el [`Context`] y la petición HTTP.
//!
//! El contexto vive en las extensiones de la request desde que la primera capa lo toca. Cada
//! middleware lo obtiene con [`ContextRequestExt::context`] y le inserta lo suyo; el handler lo
//! recibe como extractor de Axum y lo pasa hacia abajo. Como todos comparten el mismo contenido
//! (ver `Clone` en el módulo padre), da igual quién lo creó.

use std::convert::Infallible;

use axum::extract::{FromRequestParts, Request};
use axum::http::Extensions;
use axum::http::request::Parts;

use super::Context;

/// Acceso al contexto de una petición desde un middleware.
pub trait ContextRequestExt {
    /// Contexto de esta petición, creado vacío si ninguna capa anterior lo hizo.
    fn context(&mut self) -> Context;
}

impl ContextRequestExt for Extensions {
    fn context(&mut self) -> Context {
        if let Some(existing) = self.get::<Context>() {
            return existing.clone();
        }
        let fresh = Context::new();
        self.insert(fresh.clone());
        fresh
    }
}

impl ContextRequestExt for Request {
    fn context(&mut self) -> Context {
        self.extensions_mut().context()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Context {
    /// Nunca rechaza: una ruta sin middlewares recibe un contexto vacío, y es `require` quien
    /// señala, en el punto de uso, qué valor faltó.
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.context())
    }
}
