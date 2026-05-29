use async_trait::async_trait;
use crate::types::*;
use crate::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Source {
    File(PathBuf),
    Url(String),
    Database { connection_string: String, query: String, display_uri: String },
    Raw { content: Vec<u8>, mime_type: String, uri: String },
}

impl Source {
    pub fn uri(&self) -> &str {
        match self {
            Source::File(p) => p.to_str().unwrap_or(""),
            Source::Url(u) => u,
            Source::Database { display_uri, .. } => display_uri,
            Source::Raw { uri, .. } => uri,
        }
    }
}

#[async_trait]
pub trait DocumentLoader: Send + Sync {
    async fn load(&self, source: &Source) -> Result<RawDocument>;
    fn supports(&self, source: &Source) -> bool;
}

#[async_trait]
pub trait Preprocessor: Send + Sync {
    async fn process(&self, doc: RawDocument) -> Result<RawDocument>;
}

#[async_trait]
pub trait Chunker: Send + Sync {
    async fn chunk(&self, doc: &RawDocument) -> Result<Vec<Chunk>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLoader;

    #[async_trait]
    impl DocumentLoader for MockLoader {
        async fn load(&self, source: &Source) -> Result<RawDocument> {
            Ok(RawDocument {
                id: DocumentId::new(),
                content: b"hello".to_vec(),
                mime_type: "text/plain".to_string(),
                source_uri: source.uri().to_string(),
                metadata: Default::default(),
            })
        }
        fn supports(&self, source: &Source) -> bool {
            matches!(source, Source::File(_))
        }
    }

    #[test]
    fn test_database_uri_does_not_expose_credentials() {
        let source = Source::Database {
            connection_string: "postgres://admin:secret123@host:5432/mydb".to_string(),
            query: "SELECT * FROM docs".to_string(),
            display_uri: "postgres://host:5432/mydb".to_string(),
        };
        let uri = source.uri();
        assert!(!uri.contains("secret123"));
        assert!(!uri.contains("admin:"));
        assert!(uri.contains("host:5432"));
    }

    #[tokio::test]
    async fn test_loader_trait() {
        let loader = MockLoader;
        let source = Source::File("/tmp/test.txt".into());
        assert!(loader.supports(&source));
        let doc = loader.load(&source).await.unwrap();
        assert_eq!(doc.content, b"hello");
    }
}
