use arcanum_core::{Result, types::ChunkStrategyConfig, types::DocumentId, types::RawDocument};
use arcanum_ingestion::default_registry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectRequest {
    pub text:       String,
    pub strategies: Vec<ChunkStrategyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatedChunk {
    pub text:           String,
    pub char_count:     usize,
    pub token_estimate: usize,
    pub overlap_chars:  usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResult {
    pub strategy:     ChunkStrategyConfig,
    pub chunks:       Vec<AnnotatedChunk>,
    pub total_chunks: usize,
    pub mean_tokens:  f32,
}

pub async fn inspect(text: &str, strategies: &[ChunkStrategyConfig]) -> Result<Vec<InspectResult>> {
    let registry = default_registry();
    let doc = RawDocument {
        id: DocumentId::new(),
        content: text.as_bytes().to_vec(),
        mime_type: "text/plain".to_string(),
        source_uri: "inspect://input".to_string(),
        metadata: Default::default(),
    };

    let mut results = Vec::with_capacity(strategies.len());
    for config in strategies {
        let chunker = registry.build(config)?;
        let raw_chunks = chunker.chunk(&doc).await?;

        let mut prev_end = 0usize;
        let annotated: Vec<AnnotatedChunk> = raw_chunks.iter().map(|c| {
            let char_count    = c.text.chars().count();
            let token_estimate = char_count / 4;
            let overlap_chars = if c.position.start < prev_end {
                prev_end - c.position.start
            } else {
                0
            };
            prev_end = c.position.end;
            AnnotatedChunk { text: c.text.clone(), char_count, token_estimate, overlap_chars }
        }).collect();

        let total_chunks = annotated.len();
        let mean_tokens = if total_chunks == 0 {
            0.0
        } else {
            annotated.iter().map(|c| c.token_estimate as f32).sum::<f32>() / total_chunks as f32
        };

        results.push(InspectResult {
            strategy: config.clone(),
            chunks: annotated,
            total_chunks,
            mean_tokens,
        });
    }
    Ok(results)
}
