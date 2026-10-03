use atlas::demo::metrics::parse_prometheus;
use atlas::demo::{demo_router, OllamaProbe};
use atlas::samples::load_manifest;
use axum::body::Body;
use axum::Router;
use http::{Request, StatusCode};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

struct NoProbe;

#[async_trait::async_trait]
impl OllamaProbe for NoProbe {
    async fn tags(&self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
}

async fn app() -> (Router, tempfile::TempDir) {
    let (state, dir) = crate::common::test_state().await;
    app_with(state, dir)
}

/// A port nothing is listening on (bound then released).
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn app_with(state: Arc<atlas::AtlasState>, dir: tempfile::TempDir) -> (Router, tempfile::TempDir) {
    let manifest = load_manifest(&Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")).unwrap();
    (
        demo_router(state, Arc::new(manifest), Arc::new(NoProbe)),
        dir,
    )
}

async fn get(router: &Router, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().uri(uri);
    if let Some(k) = key {
        req = req.header("Authorization", format!("Bearer {k}"));
    }
    let resp = router
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn key(router: &Router) -> String {
    let (_, b) = get(router, "/demo/bootstrap", None).await;
    b["api_key"].as_str().unwrap().to_string()
}

#[test]
fn parse_counters_and_labels() {
    let s = parse_prometheus("a_total{x=\"1\",y=\"two\"} 1.5\nbare 2\n");
    assert_eq!(s.counters.len(), 2);
    assert_eq!(s.counters[0].name, "a_total");
    assert_eq!(s.counters[0].labels["x"], "1");
    assert_eq!(s.counters[0].labels["y"], "two");
    assert_eq!(s.counters[0].value, 1.5);
    assert_eq!(s.counters[1].name, "bare");
    assert!(s.counters[1].labels.is_empty());
    assert_eq!(s.counters[1].value, 2.0);
    assert!(s.histograms.is_empty());
}

#[test]
fn parse_histogram_sum_and_count_fold() {
    let text = "# TYPE lat histogram\n\
lat_bucket{op=\"s\",le=\"0.5\"} 3\n\
lat_bucket{op=\"s\",le=\"+Inf\"} 4\n\
lat_sum{op=\"s\"} 1.25\n\
lat_count{op=\"s\"} 4\n\
lat_sum{op=\"t\"} 9\n\
lat{op=\"s\",quantile=\"0.5\"} 0.1\n";
    let s = parse_prometheus(text);
    assert!(s.counters.is_empty());
    assert_eq!(s.histograms.len(), 2);
    let h = s.histograms.iter().find(|h| h.labels["op"] == "s").unwrap();
    assert_eq!(h.name, "lat");
    assert_eq!(h.count, 4.0);
    assert_eq!(h.sum, 1.25);
    let t = s.histograms.iter().find(|h| h.labels["op"] == "t").unwrap();
    assert_eq!(t.sum, 9.0);
    assert_eq!(t.count, 0.0);
}

#[test]
fn parse_ignores_comments_and_garbage() {
    let s = parse_prometheus(
        "# HELP a x\n# TYPE a counter\n\n   \nnot a metric line\nbad{ 1\nnoval{a=\"b\"}\nok 3\nx{a=\"b\"} notanumber\n",
    );
    assert_eq!(s.counters.len(), 1);
    assert_eq!(s.counters[0].name, "ok");
}

#[test]
fn parse_handles_quoted_labels_with_commas_and_braces() {
    let s = parse_prometheus("m{a=\"x,y}z\",b=\"q\\\"r\"} 7\n");
    assert_eq!(s.counters.len(), 1);
    assert_eq!(s.counters[0].labels["a"], "x,y}z");
    assert_eq!(s.counters[0].labels["b"], "q\"r");
    assert_eq!(s.counters[0].value, 7.0);
}

#[test]
fn parse_nonfinite_values_serialize() {
    let s = parse_prometheus("a NaN\nb{k=\"v\"} +Inf\nh_sum NaN\nh_count 2\n");
    assert_eq!(s.counters.len(), 2);
    assert!(s.counters[0].value.is_nan());
    assert!(s.counters[1].value.is_infinite());
    let v = serde_json::to_value(&s).unwrap();
    assert!(v["counters"][0]["value"].is_null());
    assert!(v["counters"][1]["value"].is_null());
    assert!(v["histograms"][0]["sum"].is_null());
    assert_eq!(v["histograms"][0]["count"], 2.0);
}

#[tokio::test]
async fn mcp_route_lists_seven_tools() {
    let (state, dir) = crate::common::test_state().await;
    let mcp_port = state.settings.mcp_port;
    let (router, _dir) = app_with(state, dir);
    let k = key(&router).await;
    let (st, body) = get(&router, "/demo/mcp", Some(&k)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["endpoint"], format!("http://localhost:{mcp_port}/mcp"));
    let tools = body["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in [
        "search",
        "ingest",
        "list_collections",
        "eval_run",
        "get_context",
        "generate",
        "verify",
    ] {
        assert!(names.contains(&n), "missing tool {n}: {names:?}");
    }
    for t in tools {
        assert!(!t["description"].as_str().unwrap_or("").is_empty());
        let schema = t["input_schema"].as_object().expect("input_schema object");
        assert!(!schema.is_empty());
    }
}

#[tokio::test]
async fn mcp_requires_key() {
    let (router, _dir) = app().await;
    let (st, _) = get(&router, "/demo/mcp", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn metrics_requires_key() {
    let (router, _dir) = app().await;
    let (st, _) = get(&router, "/demo/metrics", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn metrics_503_when_unreachable() {
    let (state, dir) = crate::common::test_state().await;
    let mut st = (*state).clone();
    st.settings.port = free_port();
    let (router, _dir) = app_with(Arc::new(st), dir);
    let k = key(&router).await;
    let (st, body) = get(&router, "/demo/metrics", Some(&k)).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "metrics recorder not installed");
}

#[tokio::test]
async fn metrics_proxies_when_recorder_responds() {
    let (state, dir) = crate::common::test_state().await;
    let expected = format!("Bearer {}", state.metrics_token);
    let fake = Router::new().route(
        "/metrics",
        axum::routing::get(move |headers: http::HeaderMap| {
            let expected = expected.clone();
            async move {
                if headers.get("Authorization").and_then(|v| v.to_str().ok())
                    == Some(expected.as_str())
                {
                    (
                        StatusCode::OK,
                        "# TYPE c counter\nc{a=\"b\"} 3\nh_sum 1.5\nh_count 2\n",
                    )
                } else {
                    (StatusCode::UNAUTHORIZED, "")
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, fake).await.unwrap() });
    let mut st = (*state).clone();
    st.settings.port = port;
    let (router, _dir) = app_with(Arc::new(st), dir);
    let k = key(&router).await;
    let (status, body) = get(&router, "/demo/metrics", Some(&k)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["counters"][0]["name"], "c");
    assert_eq!(body["counters"][0]["labels"]["a"], "b");
    assert_eq!(body["counters"][0]["value"], 3.0);
    assert_eq!(body["histograms"][0]["name"], "h");
    assert_eq!(body["histograms"][0]["count"], 2.0);
    assert_eq!(body["histograms"][0]["sum"], 1.5);
}
