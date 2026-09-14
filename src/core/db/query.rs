/// Resultado tipado de una consulta a base de datos que combina los elementos
/// recuperados con el total de coincidencias disponibles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QueryResult<T> {
    pub items: Vec<T>,
    pub total: u64,
}

impl<T> QueryResult<T> {
    pub fn new(items: Vec<T>, total: u64) -> Self {
        Self { items, total }
    }
}
