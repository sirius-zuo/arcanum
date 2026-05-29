use tantivy::{
    schema::{Schema, TEXT, STORED, Value},
    Index, IndexWriter, TantivyDocument,
    collector::TopDocs,
    query::QueryParser,
    ReloadPolicy,
};
use arcanum_core::{Result, ArcanumError};

pub struct Bm25Index {
    index: Index,
    id_field: tantivy::schema::Field,
    body_field: tantivy::schema::Field,
}

impl Bm25Index {
    pub fn new(path: &str) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let id_field   = schema_builder.add_text_field("id", TEXT | STORED);
        let body_field = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_dir(path, schema)
            .or_else(|_| Index::open_in_dir(path))
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        Ok(Self { index, id_field, body_field })
    }

    pub fn index_chunks(&self, chunks: Vec<(String, String)>) -> Result<()> {
        let mut writer: IndexWriter = self.index.writer(50_000_000)
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        for (id, text) in chunks {
            let mut doc = TantivyDocument::default();
            doc.add_text(self.id_field, &id);
            doc.add_text(self.body_field, &text);
            writer.add_document(doc).map_err(|e| ArcanumError::Storage(e.to_string()))?;
        }
        writer.commit().map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Returns (chunk_id, score) pairs sorted by score descending.
    pub fn search(&self, query_text: &str, top_k: usize) -> Result<Vec<(String, f32)>> {
        let reader = self.index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| ArcanumError::Storage(e.to_string()))?;
        let searcher = reader.searcher();
        let qp = QueryParser::for_index(&self.index, vec![self.body_field]);
        let query = qp.parse_query(query_text)
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(top_k))
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let mut results = vec![];
        for (score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr)
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            if let Some(id_val) = doc.get_first(self.id_field) {
                if let Some(id_str) = id_val.as_str() {
                    results.push((id_str.to_string(), score));
                }
            }
        }
        Ok(results)
    }
}
