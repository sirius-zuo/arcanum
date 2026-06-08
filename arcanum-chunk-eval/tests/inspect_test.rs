use arcanum_chunk_eval::inspect;
use arcanum_core::types::ChunkStrategyConfig;

#[tokio::test]
async fn two_strategies_produce_different_chunk_counts() {
    let text = "This is sentence one. This is sentence two. \
                This is sentence three. And a fourth sentence here.";
    let strategies = vec![
        ChunkStrategyConfig {
            strategy: "fixed".to_string(),
            params: serde_json::json!({ "chunk_size": 20, "overlap": 2 }),
        },
        ChunkStrategyConfig {
            strategy: "semantic".to_string(),
            params: serde_json::json!({ "max_chars": 200 }),
        },
    ];
    let results = inspect(text, &strategies).await.unwrap();
    assert_eq!(results.len(), 2, "should return one result per strategy");
    let fixed_count  = results[0].total_chunks;
    let semantic_count = results[1].total_chunks;
    assert_ne!(fixed_count, semantic_count,
        "fixed and semantic strategies should produce different chunk counts");
}

#[tokio::test]
async fn annotated_chunks_have_correct_char_count() {
    let text = "Hello world!";
    let strategies = vec![ChunkStrategyConfig {
        strategy: "fixed".to_string(),
        params: serde_json::json!({ "chunk_size": 100, "overlap": 0 }),
    }];
    let results = inspect(text, &strategies).await.unwrap();
    let chunk = &results[0].chunks[0];
    assert_eq!(chunk.char_count, chunk.text.chars().count());
}

#[tokio::test]
async fn unknown_strategy_returns_error() {
    let results = inspect("hello", &[ChunkStrategyConfig {
        strategy: "does-not-exist".to_string(),
        params: serde_json::json!({}),
    }]).await;
    assert!(results.is_err(), "unknown strategy should return error");
}

#[tokio::test]
async fn mean_tokens_is_approximately_char_count_over_four() {
    let text = "abcd abcd abcd abcd abcd abcd abcd abcd"; // 8 words, 4 chars each
    let strategies = vec![ChunkStrategyConfig {
        strategy: "fixed".to_string(),
        params: serde_json::json!({ "chunk_size": 100, "overlap": 0 }),
    }];
    let results = inspect(text, &strategies).await.unwrap();
    let result = &results[0];
    // mean_tokens ≈ total_chars / 4
    let expected_approx = result.chunks.iter().map(|c| c.char_count).sum::<usize>() as f32 / 4.0
        / result.total_chunks as f32;
    assert!(
        (result.mean_tokens - expected_approx).abs() < 1.0,
        "mean_tokens should approximate char_count/4: got {}, expected ~{}",
        result.mean_tokens, expected_approx
    );
}
