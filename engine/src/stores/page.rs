use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QueryResult<T> {
    pub items: Vec<T>,
    pub total: u64,
}

impl<T> QueryResult<T> {
    pub fn new(items: Vec<T>, total: u64) -> Self {
        Self { items, total }
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> QueryResult<U> {
        QueryResult { items: self.items.into_iter().map(f).collect(), total: self.total }
    }
}
