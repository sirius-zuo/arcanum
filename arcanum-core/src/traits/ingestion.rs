use async_trait::async_trait;
use crate::types::*;
use crate::types::DocumentId;
use crate::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum CloudProvider { S3, Gcs, AzureBlob }

#[derive(Debug, Clone)]
pub enum ConnectorKind { GoogleDrive, Notion, Confluence }

#[derive(Debug, Clone)]
pub enum Source {
    File(PathBuf),
    Url(String),
    Database { connection_string: String, query: String, display_uri: String },
    Raw { content: Vec<u8>, mime_hint: Option<String>, uri: String },
    CloudStorage { provider: CloudProvider, bucket: String, key: String },
    Git { repo_url: String, branch: String, path_glob: Option<String> },
    Connector { provider: ConnectorKind, resource_id: String },
}

impl Source {
    pub fn uri(&self) -> &str {
        match self {
            Source::File(p) => p.to_str().unwrap_or(""),
            Source::Url(u) => u,
            Source::Database { display_uri, .. } => display_uri,
            Source::Raw { uri, .. } => uri,
            Source::CloudStorage { .. } => "cloud://",
            Source::Git { repo_url, .. } => repo_url,
            Source::Connector { .. } => "connector://",
        }
    }

    pub fn display_uri(&self) -> String {
        match self {
            Source::CloudStorage { provider, bucket, key } => match provider {
                CloudProvider::S3 => format!("s3://{}/{}", bucket, key),
                CloudProvider::Gcs => format!("gs://{}/{}", bucket, key),
                CloudProvider::AzureBlob => format!("az://{}/{}", bucket, key),
            },
            Source::Connector { provider, resource_id } => match provider {
                ConnectorKind::GoogleDrive => format!("gdrive://{}", resource_id),
                ConnectorKind::Notion => format!("notion://{}", resource_id),
                ConnectorKind::Confluence => format!("confluence://{}", resource_id),
            },
            other => other.uri().to_string(),
        }
    }

    pub fn from_uri(uri: &str) -> Result<Self> {
        if uri.starts_with("http://") || uri.starts_with("https://") {
            return Ok(Source::Url(uri.to_string()));
        }
        if uri.starts_with("raw://") {
            return Ok(Source::Raw { content: Vec::new(), mime_hint: None, uri: uri.to_string() });
        }
        if let Some(rest) = uri.strip_prefix("s3://") {
            let (bucket, key) = split_bucket_key(rest);
            return Ok(Source::CloudStorage { provider: CloudProvider::S3, bucket, key });
        }
        if let Some(rest) = uri.strip_prefix("gs://") {
            let (bucket, key) = split_bucket_key(rest);
            return Ok(Source::CloudStorage { provider: CloudProvider::Gcs, bucket, key });
        }
        if let Some(rest) = uri.strip_prefix("az://") {
            let (bucket, key) = split_bucket_key(rest);
            return Ok(Source::CloudStorage { provider: CloudProvider::AzureBlob, bucket, key });
        }
        Ok(Source::File(PathBuf::from(uri)))
    }
}

fn split_bucket_key(rest: &str) -> (String, String) {
    match rest.find('/') {
        Some(idx) => (rest[..idx].to_string(), rest[idx + 1..].to_string()),
        None => (rest.to_string(), String::new()),
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

    /// Return structured canonical JSON (e.g. Docling blocks) for this document,
    /// if the preprocessor produces one during `process()`.
    fn canonical(&self, _doc_id: &DocumentId) -> Option<serde_json::Value> { None }

    /// Set the canonical JSON produced by `process()` for a given document.
    fn set_canonical(&self, _doc_id: &DocumentId, _canonical: serde_json::Value) {}
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

    #[test]
    fn test_source_from_uri_http() {
        let s = Source::from_uri("https://example.com/doc.pdf").unwrap();
        assert!(matches!(s, Source::Url(_)));
    }

    #[test]
    fn test_source_from_uri_file() {
        let s = Source::from_uri("/tmp/doc.pdf").unwrap();
        assert!(matches!(s, Source::File(_)));
    }

    #[test]
    fn test_source_from_uri_s3() {
        let s = Source::from_uri("s3://my-bucket/path/doc.pdf").unwrap();
        assert!(matches!(s, Source::CloudStorage { provider: CloudProvider::S3, .. }));
    }

    #[test]
    fn test_cloud_storage_uri_display() {
        let s = Source::CloudStorage {
            provider: CloudProvider::Gcs, bucket: "b".into(), key: "k/doc.pdf".into(),
        };
        assert_eq!(s.display_uri(), "gs://b/k/doc.pdf");
    }

    #[test]
    fn test_raw_mime_hint_optional() {
        let s = Source::Raw { content: b"data".to_vec(), mime_hint: None, uri: "raw://1".into() };
        assert_eq!(s.uri(), "raw://1");
    }
}
