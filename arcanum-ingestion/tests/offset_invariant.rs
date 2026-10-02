use arcanum_core::types::*;
use arcanum_ingestion::default_registry;
use serde_json::json;

const REPEATED: &str = "## A\nSame.\n## B\nSame.\n\n```\nSame\n```\nmid\n```\nSame\n```\n";

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

async fn check(text: &str, label: &str) {
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
            .chunk(&doc(text))
            .await
            .unwrap();
        assert!(!chunks.is_empty(), "{label} {name}");
        let mut last = 0;
        for c in &chunks {
            assert_eq!(
                &text[c.position.start..c.position.end],
                c.text,
                "{label} {name} chunk {}",
                c.position.index
            );
            assert!(
                c.position.start >= last,
                "{label} {name}: starts must be non-decreasing"
            );
            last = c.position.start;
        }
        // Identical chunk text must map to its own occurrence, not the first one.
        for (i, a) in chunks.iter().enumerate() {
            for b in &chunks[i + 1..] {
                if a.text == b.text {
                    assert_ne!(
                        a.position.start, b.position.start,
                        "{label} {name}: repeated text {:?} mapped to one occurrence",
                        a.text
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn offset_invariant_holds_for_every_strategy() {
    check(DOC, "lf").await;
}

#[tokio::test]
async fn offset_invariant_holds_for_crlf_documents() {
    check(&DOC.replace('\n', "\r\n"), "crlf").await;
    check(&REPEATED.replace('\n', "\r\n"), "crlf-repeated").await;
}

#[tokio::test]
async fn repeated_text_maps_to_its_own_occurrence() {
    check(REPEATED, "repeated").await;
}
