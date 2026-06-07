use arcanum_core::types::ChunkStrategyConfig;
use arcanum_ingestion::default_registry;

#[test]
fn unknown_strategy_returns_error() {
    let registry = default_registry();
    let config = ChunkStrategyConfig {
        strategy: "nonexistent".to_string(),
        params: serde_json::json!({}),
    };
    match registry.build(&config) {
        Ok(_) => panic!("expected error for unknown strategy"),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("nonexistent"), "error should name the unknown strategy: {}", msg);
        }
    }
}

#[test]
fn fixed_overlap_gte_chunk_size_returns_error() {
    let registry = default_registry();
    let config = ChunkStrategyConfig {
        strategy: "fixed".to_string(),
        params: serde_json::json!({ "chunk_size": 64, "overlap": 64 }),
    };
    match registry.build(&config) {
        Ok(_) => panic!("expected error for overlap >= chunk_size"),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("overlap"), "error should mention overlap: {}", msg);
        }
    }
}

#[test]
fn all_five_built_in_strategies_build_successfully() {
    let registry = default_registry();
    let configs = vec![
        ChunkStrategyConfig { strategy: "fixed".to_string(),
            params: serde_json::json!({ "chunk_size": 256, "overlap": 32 }) },
        ChunkStrategyConfig { strategy: "semantic".to_string(),
            params: serde_json::json!({ "max_chars": 800 }) },
        ChunkStrategyConfig { strategy: "hierarchical".to_string(),
            params: serde_json::json!({}) },
        ChunkStrategyConfig { strategy: "propositional".to_string(),
            params: serde_json::json!({}) },
        ChunkStrategyConfig { strategy: "structure".to_string(),
            params: serde_json::json!({ "max_chunk_chars": 1500 }) },
    ];
    for config in &configs {
        match registry.build(config) {
            Ok(_) => {},
            Err(e) => panic!("strategy '{}' should build, got error: {}", config.strategy, e),
        }
    }
}

#[test]
fn strategy_names_contains_all_five() {
    let registry = default_registry();
    let names = registry.strategy_names();
    for expected in &["fixed", "semantic", "hierarchical", "propositional", "structure"] {
        assert!(names.contains(&expected.to_string()), "missing strategy: {}", expected);
    }
}

#[test]
fn fixed_uses_default_params_when_not_specified() {
    let registry = default_registry();
    let config = ChunkStrategyConfig {
        strategy: "fixed".to_string(),
        params: serde_json::json!({}),  // no params — should use defaults 512/64
    };
    assert!(registry.build(&config).is_ok());
}

#[test]
fn fixed_float_chunk_size_returns_error() {
    let registry = default_registry();
    // 100.0 is an f64 literal → serde_json stores it as Float → as_u64() returns None
    let config = ChunkStrategyConfig {
        strategy: "fixed".to_string(),
        params: serde_json::json!({ "chunk_size": 100.0, "overlap": 10 }),
    };
    match registry.build(&config) {
        Ok(_) => panic!("float chunk_size should return error"),
        Err(e) => assert!(
            e.to_string().contains("chunk_size"),
            "error should name the offending param: {}",
            e
        ),
    }
}

#[test]
fn semantic_float_max_chars_returns_error() {
    let registry = default_registry();
    let config = ChunkStrategyConfig {
        strategy: "semantic".to_string(),
        params: serde_json::json!({ "max_chars": 500.0 }),
    };
    match registry.build(&config) {
        Ok(_) => panic!("float max_chars should return error"),
        Err(e) => assert!(
            e.to_string().contains("max_chars"),
            "error should name the offending param: {}",
            e
        ),
    }
}

#[test]
fn semantic_zero_max_chars_returns_error() {
    let registry = default_registry();
    let config = ChunkStrategyConfig {
        strategy: "semantic".to_string(),
        params: serde_json::json!({ "max_chars": 0 }),
    };
    match registry.build(&config) {
        Ok(_) => panic!("zero max_chars should return error"),
        Err(e) => assert!(
            e.to_string().contains("max_chars"),
            "error should mention max_chars: {}",
            e
        ),
    }
}

#[test]
fn structure_zero_max_chunk_chars_returns_error() {
    let registry = default_registry();
    let config = ChunkStrategyConfig {
        strategy: "structure".to_string(),
        params: serde_json::json!({ "max_chunk_chars": 0 }),
    };
    match registry.build(&config) {
        Ok(_) => panic!("zero max_chunk_chars should return error"),
        Err(e) => assert!(
            e.to_string().contains("max_chunk_chars"),
            "error should mention max_chunk_chars: {}",
            e
        ),
    }
}
