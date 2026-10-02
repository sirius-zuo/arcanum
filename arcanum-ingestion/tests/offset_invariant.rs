use arcanum_core::types::*;
use arcanum_ingestion::default_registry;
use serde_json::json;

const DOC: &str = "  # Intro\nCafé naïve résumé.\n\nRepeated paragraph.\n\n## Body\nRepeated paragraph.\n\nShort one! Another?\n\n```\ncode block\n```\n";

fn doc(text: &str) -> RawDocument {
    RawDocument {
        id: DocumentId::new(),
        content: text.as_bytes().to_vec(),
        mime_type: "text/plain".into(),
        source_uri: "test".into(),
        metadata: Default::default(),
    }
}

#[tokio::test]
async fn offset_invariant_holds_for_every_strategy() {
    for (name, params) in [
        ("fixed", json!({"chunk_size":24,"overlap":4})),
        ("semantic", json!({"max_chars":30})),
        ("hierarchical", json!({})),
        ("structure", json!({})),
        ("propositional", json!({})),
    ] {
        let chunks = default_registry()
            .build(&ChunkStrategyConfig {
                strategy: name.into(),
                params,
            })
            .unwrap()
            .chunk(&doc(DOC))
            .await
            .unwrap();
        assert!(!chunks.is_empty(), "{name}");
        let mut last = 0;
        for c in &chunks {
            assert_eq!(
                &DOC[c.position.start..c.position.end],
                c.text,
                "{name} chunk {}",
                c.position.index
            );
            assert!(
                c.position.start >= last,
                "{name}: starts must be non-decreasing"
            );
            last = c.position.start;
        }
    }
}
