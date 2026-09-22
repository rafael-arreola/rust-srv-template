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

    /// Transforma los elementos del resultado aplicando una función sobre cada uno.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> QueryResult<U> {
        QueryResult { items: self.items.into_iter().map(f).collect(), total: self.total }
    }
}
