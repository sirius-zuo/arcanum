use arcanum_core::{traits::{DocumentLoader, Source}, types::*, Result, ArcanumError};
use async_trait::async_trait;

pub struct RawLoader;

impl RawLoader {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl DocumentLoader for RawLoader {
    async fn load(&self, source: &Source) -> Result<RawDocument> {
        let Source::Raw { content, mime_hint, uri } = source else {
            return Err(ArcanumError::Ingestion("RawLoader only handles Source::Raw".into()));
        };
        Ok(RawDocument {
            id: DocumentId::new(),
            content: content.clone(),
            mime_type: mime_hint.clone().unwrap_or_else(|| "application/octet-stream".to_string()),
            source_uri: uri.clone(),
            metadata: Default::default(),
        })
    }

    fn supports(&self, source: &Source) -> bool {
        matches!(source, Source::Raw { .. })
    }
}
