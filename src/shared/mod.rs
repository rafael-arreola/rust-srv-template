//! Shared Kernel: vocabulario de negocio compartido entre módulos y vendors.
//!
//! Contiene exclusivamente tipos puros (enums de catálogo cerrado y value objects) con serde y
//! sus invariantes. No es una capa horizontal: aquí no hay servicios, repositorios, controladores
//! ni lógica con I/O. Reglas completas en `AGENTS.md` (Pilar 1 §1.3.10 y Pilar 2 límite 13):
//!
//! - Orden de dependencias: `core <- shared <- modules, vendors`. `shared` solo importa crates hoja
//!   (`serde`, `validator`, `chrono`, `thiserror`) y, como única excepción, `crate::core::error`.
//! - Cero tipos de persistencia (BSON, driver de MongoDB), de transporte HTTP (Axum) ni imports
//!   de módulos de dominio o vendors.
//! - Un tipo entra aquí cuando lo consumen dos módulos, o un módulo y un vendor.
//! - Representación serde estable como códigos string, idéntica en JSON (`dto.rs`) y BSON
//!   (`document.rs`), de modo que el mismo tipo atraviesa la tríada sin mappers.
//! - Enums persistidos son aditivos: jamás se renombra ni elimina una variante con datos vivos.

pub mod geo;
pub mod money;

pub use geo::Country;
pub use money::Currency;
