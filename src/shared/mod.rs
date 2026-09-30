//! Shared Kernel: vocabulario de negocio compartido entre módulos y vendors.
//!
//! Tipos puros (catálogos cerrados y value objects) con serde y sus invariantes, usados por
//! varios módulos y vendors. Solo dependen de `serde` y de `crate::core::error`: sin I/O, BSON ni
//! Axum, así que cualquier capa los importa sin arrastrar infraestructura. Su forma serde es la
//! misma en JSON (`dto.rs`) y BSON (`document.rs`) —códigos string para los enums,
//! `{ amount, currency }` para `Money`—, de modo que atraviesan transporte y almacenamiento sin
//! mappers. Un documento con una variante renombrada o eliminada deja de deserializar: para
//! retirar una variante se conserva.

pub mod geo;
pub mod money;

pub use geo::Country;
pub use money::{Currency, Money};
