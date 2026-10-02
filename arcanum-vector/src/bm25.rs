use arcanum_core::types::ChunkId;
use arcanum_core::{ArcanumError, Result};
use std::sync::Mutex;
use tantivy::{
    collector::TopDocs,
    query::{BooleanQuery, Occur, Query, QueryParser, TermQuery},
    schema::{Field, IndexRecordOption, Schema, Value, STORED, STRING, TEXT},
    Index, IndexWriter, ReloadPolicy, TantivyDocument, Term,
};
use tracing::instrument;

fn storage_err(e: impl std::fmt::Display) -> ArcanumError {
    ArcanumError::Storage(e.to_string())
}

/// Tantivy index partitioned by collection. One `IndexWriter` is shared behind a
/// mutex: Tantivy allows a single writer per index, so concurrent ingestions must
/// serialize on it rather than each opening their own.
pub struct Bm25Index {
    index: Index,
    writer: Mutex<IndexWriter>,
    id_field: Field,
    collection_field: Field,
    source_uri_field: Field,
    body_field: Field,
}

impl Bm25Index {
    pub fn new(path: &str) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let collection_field = schema_builder.add_text_field("collection", STRING);
        let source_uri_field = schema_builder.add_text_field("source_uri", STRING);
        let body_field = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_dir(path, schema)
            .or_else(|_| Index::open_in_dir(path))
            .map_err(storage_err)?;
        let writer = index.writer(50_000_000).map_err(storage_err)?;

        Ok(Self {
            index,
            writer: Mutex::new(writer),
            id_field,
            collection_field,
            source_uri_field,
            body_field,
        })
    }

    fn term_query(&self, field: Field, value: &str) -> Box<dyn Query> {
        Box::new(TermQuery::new(
            Term::from_field_text(field, value),
            IndexRecordOption::Basic,
        ))
    }

    /// Runs `f` on the shared writer. Any error rolls the writer back: after a
    /// failed commit Tantivy otherwise keeps accepting writes that never become
    /// searchable. A poisoned mutex is recovered and rolled back the same way.
    fn with_writer(
        &self,
        f: impl FnOnce(&mut IndexWriter) -> tantivy::Result<()>,
    ) -> Result<()> {
        let mut writer = self.writer.lock().unwrap_or_else(|p| {
            let mut w = p.into_inner();
            let _ = w.rollback();
            w
        });
        f(&mut writer).map_err(|e| {
            if let Err(re) = writer.rollback() {
                tracing::warn!(err = %re, "bm25 writer rollback failed");
            }
            storage_err(e)
        })
    }

    /// Adds all chunks for one source and commits once.
    pub fn index_chunks(
        &self,
        collection_id: &str,
        source_uri: &str,
        chunks: &[(ChunkId, String)],
    ) -> Result<()> {
        self.with_writer(|writer| {
            for (id, text) in chunks {
                let mut doc = TantivyDocument::default();
                doc.add_text(self.id_field, id.0.to_string());
                doc.add_text(self.collection_field, collection_id);
                doc.add_text(self.source_uri_field, source_uri);
                doc.add_text(self.body_field, text);
                writer.add_document(doc)?;
            }
            writer.commit()?;
            Ok(())
        })
    }

    /// Deletes every chunk of a source within a collection, then commits.
    pub fn delete_by_source_uri(&self, collection_id: &str, source_uri: &str) -> Result<()> {
        let query = BooleanQuery::new(vec![
            (
                Occur::Must,
                self.term_query(self.collection_field, collection_id),
            ),
            (
                Occur::Must,
                self.term_query(self.source_uri_field, source_uri),
            ),
        ]);
        self.with_writer(|writer| {
            writer.delete_query(Box::new(query))?;
            writer.commit()?;
            Ok(())
        })
    }

    /// Returns (chunk_id, score) pairs within one collection, sorted by score descending.
    pub fn search(
        &self,
        collection_id: &str,
        query_text: &str,
        top_k: usize,
    ) -> Result<Vec<(ChunkId, f32)>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e: tantivy::TantivyError| storage_err(e))?;
        let searcher = reader.searcher();
        let qp = QueryParser::for_index(&self.index, vec![self.body_field]);
        let parsed = qp.parse_query(query_text).map_err(storage_err)?;
        let query = BooleanQuery::new(vec![
            (
                Occur::Must,
                self.term_query(self.collection_field, collection_id),
            ),
            (Occur::Must, parsed),
        ]);
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(top_k))
            .map_err(storage_err)?;

        let mut results = vec![];
        for (score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr).map_err(storage_err)?;
            if let Some(id_str) = doc.get_first(self.id_field).and_then(|v| v.as_str()) {
                let uuid = uuid::Uuid::parse_str(id_str).map_err(|e| {
                    ArcanumError::Storage(format!("invalid chunk id '{id_str}' in index: {e}"))
                })?;
                results.push((ChunkId(uuid), score));
            }
        }
        Ok(results)
    }
}

#[async_trait::async_trait]
impl arcanum_core::traits::LexicalIndex for Bm25Index {
    #[instrument(skip(self), fields(index = "bm25"), err)]
    async fn search(
        &self,
        collection_id: &str,
        query: &str,
        top_k: usize,
    ) -> arcanum_core::Result<Vec<(ChunkId, f32)>> {
        self.search(collection_id, query, top_k)
    }
}
