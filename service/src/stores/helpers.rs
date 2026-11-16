use engine::db::{
    BsonExt, Collection, CollectionOptions, Database, Document, QueryResult, ReadPreference,
    SelectionCriteria, TryStreamExt, decode_entity, doc,
};
use engine::{AppResult, Cast, ObjectId, UtcDateTime};
use serde::de::DeserializeOwned;

/// Helper propio del microservicio para obtener una colección con lectura en primario
/// (consistencia lectura-tras-escritura en réplicas).
pub(super) fn primary_collection(db: &Database, name: &str) -> Collection<Document> {
    db.collection_with_options::<Document>(
        name,
        CollectionOptions::builder()
            .selection_criteria(SelectionCriteria::ReadPreference(ReadPreference::Primary))
            .build(),
    )
}

/// Ejecuta conteo y consulta paginada en paralelo decodificando cada documento hacia `E`.
pub(super) async fn paginate<E: DeserializeOwned>(
    col: &Collection<Document>,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: u32,
) -> AppResult<QueryResult<E>> {
    let (total, cursor) = tokio::try_join!(
        col.count_documents(filter.clone()),
        col.find(filter).sort(sort).skip(skip).limit(i64::from(limit))
    )?;

    let docs: Vec<Document> = cursor.try_collect().await?;
    let items = docs.into_iter().map(decode_entity).collect::<AppResult<Vec<E>>>()?;
    Ok(QueryResult::new(items, total))
}

/// Borrado lógico estándar sobre modelos con `deleted_at`.
pub(super) async fn soft_delete_by_id(
    col: &Collection<Document>,
    id: &Cast<String, ObjectId>,
    now: UtcDateTime,
) -> AppResult<bool> {
    let filter = doc! { "_id": id.to_bson()?, "deleted_at": null };
    let update = doc! { "$set": { "deleted_at": now.to_bson() } };
    let res = col.update_one(filter, update).await?;
    Ok(res.matched_count > 0)
}
