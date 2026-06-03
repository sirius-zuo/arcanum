use arcanum_core::{traits::{DocumentLoader, Source}, types::*, Result, ArcanumError};
use async_trait::async_trait;
use tracing::instrument;

pub struct HttpLoader {
    client: reqwest::Client,
}

impl HttpLoader {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }
}

#[async_trait]
impl DocumentLoader for HttpLoader {
    #[instrument(skip(self), fields(source_uri = %source.uri(), loader = "http"), err)]
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        let Source::Url(url) = source else {
            return Err(ArcanumError::Ingestion("HttpLoader only handles Source::Url".into()));
        };
        let resp = self.client.get(url).send().await
            .map_err(|e| ArcanumError::Ingestion(format!("HTTP fetch failed: {e}")))?;
        let mime_hint = resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
        let content = resp.bytes().await
            .map_err(|e| ArcanumError::Ingestion(format!("HTTP read failed: {e}")))?
            .to_vec();
        Ok(RawDocument {
            id: DocumentId::new(),
            mime_type: mime_hint.unwrap_or_else(|| "application/octet-stream".to_string()),
            source_uri: url.clone(),
            content,
            metadata: Default::default(),
        })
    }

    fn supports(&self, source: &Source) -> bool {
        matches!(source, Source::Url(_))
    }
}
