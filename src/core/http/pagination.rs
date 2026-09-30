use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::core::db::query::QueryResult;

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
    ///
    /// Se multiplica en `u64`: el producto de dos `u32` cabe siempre, mientras que multiplicar en
    /// `u32` desborda desde `page = 42_949_673` con `limit = 100` (panic en debug, página
    /// equivocada en release).
    pub fn skip(&self) -> u64 {
        u64::from(self.page().saturating_sub(1)).saturating_mul(u64::from(self.limit()))
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
    pub page: u64,
    pub limit: u64,
}

impl<T> Paginated<T> {
    /// Construye la respuesta paginada a partir del resultado de consulta y los parámetros de
    /// paginación. Deriva `total_pages` con `div_ceil` desde `total` y el `limit` ya acotado, así
    /// `page`, `limit` y `total_pages` salen siempre consistentes entre sí.
    pub fn from_query(result: QueryResult<T>, query: &PaginationQuery) -> Self {
        let limit = u64::from(query.limit());
        let total_pages = if result.total > 0 { result.total.div_ceil(limit) } else { 0 };
        Self {
            data: result.items,
            total: result.total,
            total_pages,
            page: u64::from(query.page()),
            limit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(page: Option<u32>, limit: Option<u32>) -> PaginationQuery {
        PaginationQuery { page, limit }
    }

    #[test]
    fn defaults_to_first_page_of_twenty() {
        let q = query(None, None);
        assert_eq!((q.page(), q.limit(), q.skip()), (1, 20, 0));
    }

    #[test]
    fn clamps_limit_and_page() {
        assert_eq!(query(Some(0), Some(0)).page(), 1);
        assert_eq!(query(None, Some(0)).limit(), 1);
        assert_eq!(query(None, Some(500)).limit(), 100);
    }

    #[test]
    fn skip_does_not_overflow_on_huge_pages() {
        assert_eq!(query(Some(3), Some(10)).skip(), 20);
        assert_eq!(query(Some(u32::MAX), Some(100)).skip(), u64::from(u32::MAX - 1) * 100);
    }

    #[test]
    fn total_pages_rounds_up_and_is_zero_when_empty() {
        let page =
            Paginated::from_query(QueryResult::new(vec![1, 2], 21), &query(Some(3), Some(10)));
        assert_eq!((page.total, page.total_pages, page.page, page.limit), (21, 3, 3, 10));

        let empty =
            Paginated::<u8>::from_query(QueryResult::new(Vec::new(), 0), &query(None, None));
        assert_eq!(empty.total_pages, 0);
    }
}
