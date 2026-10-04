use super::{require_key, DemoCtx, DemoError};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct Counter {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    /// Non-finite values (`NaN`, `+Inf`) serialize as JSON `null`.
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Histogram {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub count: f64,
    pub sum: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MetricsSnapshot {
    pub counters: Vec<Counter>,
    pub histograms: Vec<Histogram>,
}

/// Parses `{k="v",...}` starting just after the `{`. Returns the labels and the rest of the
/// line after the closing `}`. Quoted values may contain `,`, `}` and backslash escapes.
fn parse_labels(s: &str) -> Option<(BTreeMap<String, String>, &str)> {
    let mut labels = BTreeMap::new();
    let mut rest = s;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if let Some(after) = rest.strip_prefix('}') {
            return Some((labels, after));
        }
        let eq = rest.find('=')?;
        let key = rest[..eq].trim().to_string();
        if key.is_empty() {
            return None;
        }
        rest = rest[eq + 1..].trim_start().strip_prefix('"')?;
        let mut value = String::new();
        let mut chars = rest.char_indices();
        let end = loop {
            let (i, c) = chars.next()?;
            match c {
                '"' => break i,
                '\\' => match chars.next()?.1 {
                    'n' => value.push('\n'),
                    other => value.push(other),
                },
                c => value.push(c),
            }
        };
        labels.insert(key, value);
        rest = &rest[end + 1..];
    }
}

/// Returns `(name, labels, value)` for one sample line, or `None` if it is malformed.
fn parse_line(line: &str) -> Option<(String, BTreeMap<String, String>, f64)> {
    let name_end = line.find(|c: char| c == '{' || c.is_whitespace())?;
    let name = &line[..name_end];
    if name.is_empty() {
        return None;
    }
    let rest = &line[name_end..];
    let (labels, rest) = match rest.strip_prefix('{') {
        Some(inner) => parse_labels(inner)?,
        None => (BTreeMap::new(), rest),
    };
    let value = rest.split_whitespace().next()?.parse::<f64>().ok()?;
    Some((name.to_string(), labels, value))
}

/// Parses Prometheus text exposition into counters and histogram sum/count pairs. Bucket and
/// quantile lines, comments and malformed lines are skipped.
pub fn parse_prometheus(text: &str) -> MetricsSnapshot {
    let mut snap = MetricsSnapshot::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, labels, value)) = parse_line(line) else {
            continue;
        };
        if name.ends_with("_bucket") || labels.contains_key("quantile") {
            continue;
        }
        let (base, is_sum) = if let Some(b) = name.strip_suffix("_sum") {
            (b, true)
        } else if let Some(b) = name.strip_suffix("_count") {
            (b, false)
        } else {
            snap.counters.push(Counter {
                name,
                labels,
                value,
            });
            continue;
        };
        let idx = match snap
            .histograms
            .iter()
            .position(|h| h.name == base && h.labels == labels)
        {
            Some(i) => i,
            None => {
                snap.histograms.push(Histogram {
                    name: base.to_string(),
                    labels,
                    count: 0.0,
                    sum: 0.0,
                });
                snap.histograms.len() - 1
            }
        };
        if is_sum {
            snap.histograms[idx].sum = value;
        } else {
            snap.histograms[idx].count = value;
        }
    }
    snap
}

const UNAVAILABLE: &str = "the engine returned no metrics";

/// `GET /demo/metrics`: proxies the engine's token-protected `/metrics` as JSON.
pub async fn metrics(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<Json<MetricsSnapshot>, DemoError> {
    require_key(&ctx.state, &headers)?;
    let unavailable = || DemoError::Unavailable(UNAVAILABLE.into());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| unavailable())?;
    let resp = client
        .get(format!(
            "http://127.0.0.1:{}/metrics",
            ctx.state.settings.port
        ))
        .bearer_auth(&ctx.state.metrics_token)
        .send()
        .await
        .map_err(|_| unavailable())?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(unavailable());
    }
    let body = resp.text().await.map_err(|_| unavailable())?;
    if body.trim().is_empty() {
        return Err(unavailable());
    }
    Ok(Json(parse_prometheus(&body)))
}
