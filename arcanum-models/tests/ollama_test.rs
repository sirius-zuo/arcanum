use arcanum_models::OllamaProvider;
use arcanum_core::traits::*;
use arcanum_core::types::*;
use mockito::Server;

#[tokio::test]
async fn test_ollama_embed() {
    let mut server = Server::new_async().await;
    let mock = server.mock("POST", "/api/embeddings")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"embedding": [0.1, 0.2, 0.3]}"#)
        .create_async().await;

    let provider = OllamaProvider::new(&server.url(), "nomic-embed-text", "qwen2.5:7b");
    let vecs = provider.embed(vec!["hello".to_string()]).await.unwrap();
    assert_eq!(vecs.len(), 1);
    assert_eq!(vecs[0].0.len(), 3);
    mock.assert_async().await;
}

#[tokio::test]
async fn test_ollama_enrich_context_prefix() {
    let mut server = Server::new_async().await;
    let mock = server.mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response": "This chunk is about Rust programming."}"#)
        .create_async().await;

    let provider = OllamaProvider::new(&server.url(), "nomic-embed-text", "qwen2.5:7b");
    let result = provider.enrich(EnrichRequest {
        text: "ownership and borrowing".to_string(),
        intent: EnrichIntent::ContextPrefix,
        context: None,
    }).await.unwrap();
    assert!(result.0.contains("Rust"));
    mock.assert_async().await;
}
