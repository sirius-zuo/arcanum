use arcanum_chunk_eval::benchmark::{BenchmarkJob, LabeledQuery, run_benchmark};
use arcanum_core::types::{ChunkStrategyConfig, DocumentId, RawDocument};

fn make_doc(text: &str) -> RawDocument {
    RawDocument {
        id: DocumentId::new(),
        content: text.as_bytes().to_vec(),
        mime_type: "text/plain".to_string(),
        source_uri: format!("test://{}", text.len()),
        metadata: Default::default(),
    }
}

#[tokio::test]
async fn benchmark_returns_one_metric_per_strategy() {
    let doc = make_doc("The quick brown fox jumps over the lazy dog.");
    let doc_id = doc.id.clone();
    let job = BenchmarkJob {
        corpus: vec![doc],
        queries: vec![LabeledQuery {
            text: "fox".to_string(),
            expected_doc_ids: vec![doc_id],
        }],
        strategies: vec![
            ChunkStrategyConfig {
                strategy: "fixed".to_string(),
                params: serde_json::json!({ "chunk_size": 20, "overlap": 2 }),
            },
            ChunkStrategyConfig {
                strategy: "semantic".to_string(),
                params: serde_json::json!({ "max_chars": 200 }),
            },
        ],
    };
    let results = run_benchmark(job).await.unwrap();
    assert_eq!(results.len(), 2, "should return one BenchmarkMetrics per strategy");
}

#[tokio::test]
async fn deterministic_chunker_produces_stable_recall() {
    let doc = make_doc("Rust is a systems programming language. It is fast and safe.");
    let doc_id = doc.id.clone();

    let strategy = ChunkStrategyConfig {
        strategy: "fixed".to_string(),
        params: serde_json::json!({ "chunk_size": 30, "overlap": 5 }),
    };
    let job = BenchmarkJob {
        corpus: vec![doc.clone()],
        queries: vec![LabeledQuery {
            text: "Rust programming".to_string(),
            expected_doc_ids: vec![doc_id.clone()],
        }],
        strategies: vec![strategy.clone()],
    };

    let results1 = run_benchmark(job.clone()).await.unwrap();
    let results2 = run_benchmark(job).await.unwrap();

    assert_eq!(
        (results1[0].recall_at_5 * 100.0) as i32,
        (results2[0].recall_at_5 * 100.0) as i32,
        "same corpus + strategy + queries should produce identical recall"
    );
}

#[tokio::test]
async fn recall_is_between_zero_and_one() {
    let doc = make_doc("Some text content for testing purposes here.");
    let doc_id = doc.id.clone();
    let job = BenchmarkJob {
        corpus: vec![doc],
        queries: vec![LabeledQuery {
            text: "testing".to_string(),
            expected_doc_ids: vec![doc_id],
        }],
        strategies: vec![ChunkStrategyConfig {
            strategy: "fixed".to_string(),
            params: serde_json::json!({ "chunk_size": 50, "overlap": 5 }),
        }],
    };
    let results = run_benchmark(job).await.unwrap();
    assert!(results[0].recall_at_5 >= 0.0 && results[0].recall_at_5 <= 1.0);
    assert!(results[0].recall_at_10 >= 0.0 && results[0].recall_at_10 <= 1.0);
}
