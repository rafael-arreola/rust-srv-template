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
        (u64::from(self.page()) - 1) * u64::from(self.limit())
    }
}

impl Default for PaginationQuery {
    fn default() -> Self {
        Self { page: Some(1), limit: Some(20) }
    }
}

/// Contenedor genérico universal de paginación para respuestas HTTP.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Paginated<T> {
    pub data: Vec<T>,
    pub total: u64,
    pub total_pages: u64,
    pub page: u32,
    pub limit: u32,
}

impl<T> Paginated<T> {
    pub fn new(data: Vec<T>, total: u64, page: u32, limit: u32) -> Self {
        let total_pages =
            if limit > 0 && total > 0 { (total as u128).div_ceil(limit as u128) as u64 } else { 0 };
        Self { data, total, total_pages, page, limit }
    }
}

/// Alias canónico para la respuesta paginada.
pub type PaginatedResponse<T> = Paginated<T>;

/// Alias de compatibilidad retrocompatible.
pub type GenericPagination<T> = Paginated<T>;
