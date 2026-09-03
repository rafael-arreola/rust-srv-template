use serde::{Deserialize, Serialize};
use validator::Validate;

/// Parámetros canónicos de paginación para endpoints HTTP y consultas de base de datos.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct PaginationQuery {
    #[validate(range(min = 1, message = "Page must be greater than or equal to 1"))]
    pub page: Option<u32>,

    #[validate(range(min = 1, max = 100, message = "Limit must be between 1 and 100"))]
    pub limit: Option<u32>,
}

impl PaginationQuery {
    /// Número de página (1-indexed). Por defecto 1.
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    /// Límite de elementos por página acotado entre 1 y 100. Por defecto 20.
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(20).clamp(1, 100)
    }

    /// Desplazamiento (skip/offset) para motores de base de datos (MongoDB/SQL).
    pub fn skip(&self) -> u64 {
        ((self.page() - 1) * self.limit()) as u64
    }
}

impl Default for PaginationQuery {
    fn default() -> Self {
        Self { page: Some(1), limit: Some(20) }
    }
}
