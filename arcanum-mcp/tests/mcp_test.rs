use arcanum_mcp::McpJsonRpcHandler;
use serde_json::json;

#[tokio::test]
async fn test_mcp_list_tools_response() {
    let handler = McpJsonRpcHandler::new_test();
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    let resp = handler.handle(req, axum::http::HeaderMap::new()).await.unwrap();
    assert_eq!(resp["jsonrpc"], "2.0");
    assert!(resp["result"]["tools"].is_array());
    let tools: Vec<String> = resp["result"]["tools"]
        .as_array().unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(tools.contains(&"ingest".to_string()));
    assert!(tools.contains(&"search".to_string()));
}

#[tokio::test]
async fn test_mcp_unknown_method_returns_error() {
    let handler = McpJsonRpcHandler::new_test();
    let req = json!({ "jsonrpc": "2.0", "id": 2, "method": "unknown/method", "params": {} });
    let resp = handler.handle(req, axum::http::HeaderMap::new()).await.unwrap();
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32601);
}
