//! Contexto de operación: la bolsa tipada que acompaña una petición desde el middleware hasta la
//! última función que necesita saber algo de ella.
//!
//! Cumple el papel de `context.Context` en Go para **transportar valores**, no para cancelar: un
//! future en Rust se cancela al soltarlo, y el presupuesto de tiempo lo impone `request_timeout`.
//!
//! Guarda **un valor por tipo**, y el tipo es la clave: `ctx.insert(country)` y después
//! `ctx.require::<Country>()?`. El tipo nombra el dato, así que un primitivo que necesite nombre
//! propio se modela como newtype (`struct RequestId(String)`) y nunca entra desnudo a la bolsa.
//!
//! - `insert`, `get` y `require` reciben `&self`: cualquier capa puede meter o sacar valores sin
//!   pedir `&mut`, y varios futures pueden compartir el mismo contexto a la vez.
//! - `Clone` devuelve un handle al **mismo** contenido, como pasar `ctx` por valor en Go; sirve
//!   porque da igual qué capa lo creó. Una tarea que corre desprendida arma su propia bolsa desde
//!   el payload que la despertó (`Context::new().with(country).with(actor)`).
//! - `core` no conoce los tipos de negocio: el contexto guarda lo que las capas superiores decidan
//!   (país, actor, correlación) y cada una lee solo lo que declara necesitar.

mod extract;

pub use extract::ContextRequestExt;

use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::core::error::{AppError, AppResult};

/// Requisitos de todo valor que viaja en el contexto.
///
/// `Clone` es necesario porque los valores viven detrás de un candado y se entregan por copia;
/// un valor grande o costoso de clonar se guarda envuelto en `Arc`.
pub trait ContextValue: Any + Clone + Send + Sync + 'static {}

impl<T> ContextValue for T where T: Any + Clone + Send + Sync + 'static {}

/// Bolsa de valores borrados, cada uno bajo su [`TypeId`]. El `Arc` permite clonar el mapa entero
/// al copiar el `Context` sin conocer los tipos concretos que contiene.
type Values = HashMap<TypeId, Arc<dyn Any + Send + Sync>>;

/// Bolsa tipada de valores de la operación en curso. Ver la documentación del módulo.
#[derive(Clone, Default)]
pub struct Context {
    values: Arc<RwLock<Values>>,
}

impl Context {
    /// Contexto vacío. Equivale a `context.Background()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructor encadenable: `Context::new().with(country).with(actor)`.
    ///
    /// Pensado para quien arma el contexto fuera de HTTP, como un worker que lo reconstruye desde
    /// el payload de la tarea.
    #[must_use]
    pub fn with<T: ContextValue>(self, value: T) -> Self {
        self.insert(value);
        self
    }

    /// Guarda `value` bajo su tipo y devuelve el valor anterior de ese mismo tipo, si lo había.
    pub fn insert<T: ContextValue>(&self, value: T) -> Option<T> {
        self.write()
            .insert(TypeId::of::<T>(), Arc::new(value))
            .and_then(|previous| previous.downcast_ref::<T>().cloned())
    }

    /// Copia del valor de tipo `T`, si alguna capa lo insertó.
    pub fn get<T: ContextValue>(&self) -> Option<T> {
        self.read().get(&TypeId::of::<T>()).and_then(|value| value.downcast_ref::<T>()).cloned()
    }

    /// Como [`get`](Self::get), pero la ausencia es un defecto de cableado: la capa que debía
    /// insertar `T` no corrió para esta ruta. Se reporta como error interno, nunca como 4xx.
    pub fn require<T: ContextValue>(&self) -> AppResult<T> {
        self.get::<T>().ok_or_else(|| {
            AppError::internal(format!(
                "operation context has no value of type `{}`",
                type_name::<T>()
            ))
        })
    }

    /// Un candado envenenado solo significa que otro hilo entró en pánico a mitad de una
    /// escritura; la bolsa sigue siendo legible y no hay invariante que proteger.
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
