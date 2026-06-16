use std::collections::HashMap;
use std::io::Write as IoWrite;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use arcanum_core::{traits::Preprocessor, types::{DocumentId, RawDocument}, ArcanumError, Result};
use async_trait::async_trait;

/// Shared response shape for both HTTP and async Docling backends.
#[derive(serde::Deserialize)]
struct ConvertResponse {
    document: ConvertedDoc,
}

#[derive(serde::Deserialize)]
struct ConvertedDoc {
    #[serde(default)]
    md_content: Option<String>,
    #[serde(default)]
    metadata:   Option<serde_json::Value>,
}

pub enum DoclingBackend {
    Http {
        base_url: String,
        api_key: Option<String>,
        timeout_secs: u64,
        use_async: bool,
        poll_interval_ms: u64,
    },
    Cli {
        command: String,
    },
}

pub struct DoclingPreprocessor {
    backend:    DoclingBackend,
    client:     reqwest::Client,
    canonicals: RwLock<HashMap<DocumentId, serde_json::Value>>,
}

impl DoclingPreprocessor {
    pub fn new(backend: DoclingBackend) -> Self {
        Self {
            backend,
            client: reqwest::Client::new(),
            canonicals: RwLock::new(HashMap::new()),
        }
    }

    /// Extract Docling canonical JSON from the response body string.
    fn extract_canonical_from_str(&self, body: &str) -> Option<serde_json::Value> {
        let resp: ConvertResponse = serde_json::from_str(body)
            .map_err(|e| tracing::debug!("docling response does not match expected shape; canonical will be None: {e}"))
            .ok()?;

        // Build a minimal canonical from the metadata if available.
        resp.document.metadata.map(|m| {
            let mut canonical = serde_json::Map::new();
            if let Some(md) = m.as_object() {
                canonical.insert("blocks".to_string(), md.get("blocks").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![])));
            }
            serde_json::Value::Object(canonical)
        })
    }

    /// Extract markdown from the response body string.
    fn extract_md_from_str(body: &str) -> Result<String> {
        let resp: ConvertResponse = serde_json::from_str(body)
            .map_err(|e| ArcanumError::Ingestion(format!("docling response parse error: {e}")))?;

        let md = resp.document.md_content.ok_or_else(|| {
            ArcanumError::Ingestion("docling response missing md_content".into())
        })?;

        if md.is_empty() {
            return Err(ArcanumError::Ingestion(
                "docling produced empty markdown output".into(),
            ));
        }

        Ok(md)
    }
}

use arcanum_core::config::DoclingBackendConfig;

impl From<&DoclingBackendConfig> for DoclingBackend {
    fn from(cfg: &DoclingBackendConfig) -> Self {
        match cfg {
            DoclingBackendConfig::Http {
                base_url,
                api_key,
                timeout_secs,
                use_async,
                poll_interval_ms,
            } => DoclingBackend::Http {
                base_url: base_url.clone(),
                api_key: api_key.clone(),
                timeout_secs: *timeout_secs,
                use_async: *use_async,
                poll_interval_ms: *poll_interval_ms,
            },
            DoclingBackendConfig::Cli { command } => DoclingBackend::Cli {
                command: command.clone(),
            },
        }
    }
}

const SUPPORTED_MIMES: &[&str] = &[
    "application/pdf",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/epub+zip",
    "text/html",
    "application/xhtml+xml",
    "image/png",
    "image/jpeg",
    "image/tiff",
];

fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        "application/pdf" => "pdf",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => "pptx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/epub+zip" => "epub",
        "text/html" | "application/xhtml+xml" => "html",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/tiff" => "tiff",
        _ => "bin",
    }
}

impl DoclingPreprocessor {
    async fn convert_via_http(
        &self,
        doc: RawDocument,
        base_url: &str,
        api_key: &Option<String>,
        timeout_secs: u64,
        use_async: bool,
        poll_interval_ms: u64,
    ) -> Result<RawDocument> {
        let filename = std::path::Path::new(&doc.source_uri)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("document")
            .to_string();

        // Set deadline before building the request so the POST and any
        // subsequent polling share a single total time budget.
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);

        let mut doc = doc;
        let content = std::mem::take(&mut doc.content);

        let file_part = reqwest::multipart::Part::bytes(content)
            .file_name(filename)
            .mime_str(&doc.mime_type)
            .map_err(|e| ArcanumError::Ingestion(format!("multipart MIME error: {e}")))?;

        let form = reqwest::multipart::Form::new()
            .part("files", file_part)
            .text("to_formats", "md");

        let endpoint = if use_async {
            format!("{base_url}/v1/convert/file/async")
        } else {
            format!("{base_url}/v1/convert/file")
        };

        let budget = deadline.saturating_duration_since(Instant::now());
        let mut req = self
            .client
            .post(&endpoint)
            .timeout(budget)
            .multipart(form);

        if let Some(key) = api_key {
            req = req.header("X-Api-Key", key.as_str());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ArcanumError::Ingestion(format!("docling HTTP request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ArcanumError::Ingestion(format!(
                "docling-serve returned {status}: {body}"
            )));
        }

        if use_async {
            self.poll_and_fetch(resp, base_url, api_key, deadline, poll_interval_ms, doc)
                .await
        } else {
            // Read the response body once, then parse both canonical and markdown.
            let body = resp
                .text()
                .await
                .map_err(|e| ArcanumError::Ingestion(format!("docling response body error: {e}")))?;
            let md = Self::extract_md_from_str(&body)?;
            let canonical = self.extract_canonical_from_str(&body);
            if let Some(ref canon) = canonical {
                if let Some(mut w) = self.canonicals.write().ok() {
                    let _ = w.insert(doc.id.clone(), canon.clone());
                }
            }
            Ok(RawDocument {
                content: md.into_bytes(),
                mime_type: "text/markdown".to_string(),
                ..doc
            })
        }
    }

    async fn poll_and_fetch(
        &self,
        submit_resp: reqwest::Response,
        base_url: &str,
        api_key: &Option<String>,
        deadline: Instant,
        poll_interval_ms: u64,
        doc: RawDocument,
    ) -> Result<RawDocument> {
        #[derive(serde::Deserialize)]
        struct SubmitResponse {
            task_id: String,
        }
        #[derive(serde::Deserialize)]
        struct PollResponse {
            task_status: String,
            #[serde(default)]
            error_message: Option<String>,
        }

        let submit: SubmitResponse = submit_resp
            .json()
            .await
            .map_err(|e| ArcanumError::Ingestion(format!("docling async submit parse error: {e}")))?;

        let task_id = submit.task_id;

        loop {
            // Sleep first so the first poll fires after poll_interval_ms, not immediately.
            tokio::time::sleep(Duration::from_millis(poll_interval_ms)).await;

            // Check deadline AFTER sleep so we report a clean timeout even when
            // the sleep itself pushes past the deadline.
            if Instant::now() > deadline {
                return Err(ArcanumError::Ingestion(format!(
                    "docling async conversion timed out (task {task_id})"
                )));
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut poll_req = self
                .client
                .get(format!("{base_url}/v1/status/poll/{task_id}"))
                .timeout(remaining);
            if let Some(key) = api_key {
                poll_req = poll_req.header("X-Api-Key", key.as_str());
            }
            let poll_resp = poll_req
                .send()
                .await
                .map_err(|e| ArcanumError::Ingestion(format!("docling poll request failed: {e}")))?;

            if !poll_resp.status().is_success() {
                let status = poll_resp.status().as_u16();
                let body = poll_resp.text().await.unwrap_or_default();
                return Err(ArcanumError::Ingestion(format!(
                    "docling poll returned {status}: {body}"
                )));
            }

            let poll: PollResponse = poll_resp
                .json()
                .await
                .map_err(|e| ArcanumError::Ingestion(format!("docling poll parse error: {e}")))?;

            match poll.task_status.as_str() {
                "success" => break,
                "failure" => {
                    let msg = poll
                        .error_message
                        .unwrap_or_else(|| "unknown error".into());
                    return Err(ArcanumError::Ingestion(format!(
                        "docling async conversion failed: {msg}"
                    )));
                }
                "pending" | "started" => { /* keep polling */ }
                other => {
                    return Err(ArcanumError::Ingestion(format!(
                        "docling poll returned unexpected status '{other}' (task {task_id})"
                    )));
                }
            }
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        let mut result_req = self
            .client
            .get(format!("{base_url}/v1/result/{task_id}"))
            .timeout(remaining);
        if let Some(key) = api_key {
            result_req = result_req.header("X-Api-Key", key.as_str());
        }
        let result_resp = result_req
            .send()
            .await
            .map_err(|e| ArcanumError::Ingestion(format!("docling result fetch failed: {e}")))?;

        if !result_resp.status().is_success() {
            let status = result_resp.status().as_u16();
            let body = result_resp.text().await.unwrap_or_default();
            return Err(ArcanumError::Ingestion(format!(
                "docling result fetch returned {status}: {body}"
            )));
        }

        let body = result_resp
            .text()
            .await
            .map_err(|e| ArcanumError::Ingestion(format!("docling result body error: {e}")))?;
        let md = Self::extract_md_from_str(&body)?;
        let canonical = self.extract_canonical_from_str(&body);
        if let Some(ref canon) = canonical {
            if let Some(mut w) = self.canonicals.write().ok() {
                let _ = w.insert(doc.id.clone(), canon.clone());
            }
        }
        Ok(RawDocument {
            content: md.into_bytes(),
            mime_type: "text/markdown".to_string(),
            ..doc
        })
    }

    async fn convert_via_cli(&self, doc: RawDocument, command: &str) -> Result<RawDocument> {
        let ext = mime_to_ext(&doc.mime_type);

        let mut input_file = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile()
            .map_err(|e| ArcanumError::Ingestion(format!("failed to create temp file: {e}")))?;

        IoWrite::write_all(&mut input_file, &doc.content)
            .map_err(|e| ArcanumError::Ingestion(format!("failed to write temp file: {e}")))?;
        IoWrite::flush(&mut input_file)
            .map_err(|e| ArcanumError::Ingestion(format!("failed to flush temp file: {e}")))?;

        let output_dir = tempfile::TempDir::new()
            .map_err(|e| ArcanumError::Ingestion(format!("failed to create temp dir: {e}")))?;

        let input_path = input_file.path().to_path_buf();
        let stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ArcanumError::Ingestion("invalid temp file stem".into()))?
            .to_string();
        let output_path = output_dir.path().join(format!("{stem}.md"));

        let input_path_str = input_path
            .to_str()
            .ok_or_else(|| ArcanumError::Ingestion("temp input path is not valid UTF-8".into()))?;
        let output_dir_str = output_dir
            .path()
            .to_str()
            .ok_or_else(|| ArcanumError::Ingestion("temp output dir path is not valid UTF-8".into()))?;

        let output = tokio::process::Command::new(command)
            .args([
                "convert",
                input_path_str,
                "--to",
                "md",
                "--output",
                output_dir_str,
            ])
            .output()
            .await
            .map_err(|e| ArcanumError::Ingestion(format!("failed to spawn docling CLI: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ArcanumError::Ingestion(format!(
                "docling CLI exited {}: {stderr}",
                output.status.code().unwrap_or(-1)
            )));
        }

        let md_bytes = tokio::fs::read(&output_path).await.map_err(|e| {
            ArcanumError::Ingestion(format!(
                "failed to read docling output at {}: {e}",
                output_path.display()
            ))
        })?;

        if md_bytes.is_empty() {
            return Err(ArcanumError::Ingestion(
                "docling CLI produced empty markdown output".into(),
            ));
        }

        Ok(RawDocument {
            content: md_bytes,
            mime_type: "text/markdown".to_string(),
            ..doc
        })
    }
}

#[async_trait]
impl Preprocessor for DoclingPreprocessor {
    async fn process(&self, doc: RawDocument) -> Result<RawDocument> {
        if !SUPPORTED_MIMES.contains(&doc.mime_type.as_str()) {
            return Ok(doc);
        }
        match &self.backend {
            DoclingBackend::Http {
                base_url,
                api_key,
                timeout_secs,
                use_async,
                poll_interval_ms,
            } => {
                self.convert_via_http(
                    doc,
                    base_url,
                    api_key,
                    *timeout_secs,
                    *use_async,
                    *poll_interval_ms,
                )
                .await
            }
            DoclingBackend::Cli { command } => self.convert_via_cli(doc, command).await,
        }
    }

    fn canonical(&self, doc_id: &DocumentId) -> Option<serde_json::Value> {
        // write() to remove the entry after reading (single-use eviction, prevents unbounded growth).
        self.canonicals.write().ok()?.remove(doc_id)
    }

    fn set_canonical(&self, doc_id: &DocumentId, canonical: serde_json::Value) {
        if let Some(mut w) = self.canonicals.write().ok() {
            let _ = w.insert(doc_id.clone(), canonical);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::DocumentId;

    fn raw_doc(content: Vec<u8>, mime_type: &str) -> RawDocument {
        RawDocument {
            id: DocumentId::new(),
            content,
            mime_type: mime_type.into(),
            source_uri: "test://doc.pdf".into(),
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_passthrough_unsupported_mime() {
        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: "http://localhost:9999".into(),
            api_key: None,
            timeout_secs: 5,
            use_async: false,
            poll_interval_ms: 2000,
        });
        let doc = raw_doc(b"plain text".to_vec(), "text/plain");
        let out = p.process(doc).await.unwrap();
        assert_eq!(out.mime_type, "text/plain");
        assert_eq!(out.content, b"plain text");
    }

    #[tokio::test]
    async fn test_passthrough_octet_stream() {
        let p = DoclingPreprocessor::new(DoclingBackend::Cli {
            command: "false".into(),
        });
        let doc = raw_doc(b"binary".to_vec(), "application/octet-stream");
        let out = p.process(doc).await.unwrap();
        assert_eq!(out.mime_type, "application/octet-stream");
    }

    #[tokio::test]
    async fn test_http_backend_converts_pdf_to_markdown() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/convert/file"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "document": { "md_content": "# Hello\n\nWorld." },
                "status": "success"
            })))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: None,
            timeout_secs: 10,
            use_async: false,
            poll_interval_ms: 2000,
        });
        let doc = raw_doc(b"%PDF-1.4 fake".to_vec(), "application/pdf");
        let out = p.process(doc).await.unwrap();
        assert_eq!(out.mime_type, "text/markdown");
        assert_eq!(String::from_utf8(out.content).unwrap(), "# Hello\n\nWorld.");
    }

    #[tokio::test]
    async fn test_http_backend_sends_api_key_header() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path, header};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/convert/file"))
            .and(header("X-Api-Key", "secret-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "document": { "md_content": "# Doc" },
                "status": "success"
            })))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: Some("secret-key".into()),
            timeout_secs: 10,
            use_async: false,
            poll_interval_ms: 2000,
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        let out = p.process(doc).await.unwrap();
        assert_eq!(out.mime_type, "text/markdown");
    }

    #[tokio::test]
    async fn test_http_backend_error_on_non_2xx() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/convert/file"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: None,
            timeout_secs: 10,
            use_async: false,
            poll_interval_ms: 2000,
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        let err = p.process(doc).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("500"), "error should mention status code: {msg}");
    }

    #[tokio::test]
    async fn test_http_backend_error_on_empty_md_content() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/convert/file"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "document": { "md_content": "" },
                "status": "failure"
            })))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: None,
            timeout_secs: 10,
            use_async: false,
            poll_interval_ms: 2000,
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        assert!(p.process(doc).await.is_err());
    }

    #[tokio::test]
    async fn test_http_async_backend_polls_until_success() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;

        // Submit endpoint returns task_id
        Mock::given(method("POST"))
            .and(path("/v1/convert/file/async"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-abc",
                "task_status": "pending"
            })))
            .mount(&server)
            .await;

        // First poll: still pending
        Mock::given(method("GET"))
            .and(path("/v1/status/poll/task-abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-abc",
                "task_status": "started"
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second poll: success
        Mock::given(method("GET"))
            .and(path("/v1/status/poll/task-abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-abc",
                "task_status": "success"
            })))
            .mount(&server)
            .await;

        // Result fetch
        Mock::given(method("GET"))
            .and(path("/v1/result/task-abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "document": { "md_content": "# Async Result" },
                "status": "success"
            })))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: None,
            timeout_secs: 30,
            use_async: true,
            poll_interval_ms: 50, // fast polling for test
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        let out = p.process(doc).await.unwrap();
        assert_eq!(out.mime_type, "text/markdown");
        assert_eq!(String::from_utf8(out.content).unwrap(), "# Async Result");
    }

    #[tokio::test]
    async fn test_http_async_backend_error_on_failure_status() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/convert/file/async"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-fail", "task_status": "pending"
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/status/poll/task-fail"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-fail",
                "task_status": "failure",
                "error_message": "unsupported encoding"
            })))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: None,
            timeout_secs: 30,
            use_async: true,
            poll_interval_ms: 50,
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        let err = p.process(doc).await.unwrap_err();
        assert!(err.to_string().contains("unsupported encoding"), "got: {err}");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cli_backend_converts_pdf_to_markdown() {
        use std::os::unix::fs::PermissionsExt;

        // Create a stub script that mimics `docling convert`
        // Args: convert <input_file> --to md --output <output_dir>
        // ($1=convert $2=input_file $3=--to $4=md $5=--output $6=output_dir)
        let script = tempfile::Builder::new().suffix(".sh").tempfile().unwrap();
        let script_path = script.path().to_path_buf();
        std::fs::write(&script_path, b"#!/bin/sh\nINPUT=\"$2\"\nOUTDIR=\"$6\"\nSTEM=$(basename \"${INPUT%.*}\")\nmkdir -p \"$OUTDIR\"\nprintf '# Stub Heading\\n\\nStub body.\\n' > \"$OUTDIR/$STEM.md\"\n").unwrap();
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let p = DoclingPreprocessor::new(DoclingBackend::Cli {
            command: script_path.to_str().unwrap().to_string(),
        });
        let doc = raw_doc(b"%PDF-1.4 fake content".to_vec(), "application/pdf");
        let out = p.process(doc).await.unwrap();
        assert_eq!(out.mime_type, "text/markdown");
        let text = String::from_utf8(out.content).unwrap();
        assert!(text.contains("# Stub Heading"), "expected heading: got {text:?}");
        assert!(text.contains("Stub body."), "expected body: got {text:?}");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cli_backend_error_on_nonzero_exit() {
        let p = DoclingPreprocessor::new(DoclingBackend::Cli {
            command: "false".into(), // always exits 1
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        let err = p.process(doc).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("exited") || msg.contains("failed") || msg.contains("spawn"),
            "error message should describe failure: {msg}"
        );
    }

    #[tokio::test]
    async fn test_http_async_poll_non_2xx_returns_error_with_status() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/convert/file/async"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-poll-err", "task_status": "pending"
            })))
            .mount(&server)
            .await;
        // Poll endpoint returns 503
        Mock::given(method("GET"))
            .and(path("/v1/status/poll/task-poll-err"))
            .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: None,
            timeout_secs: 10,
            use_async: true,
            poll_interval_ms: 50,
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        let err = p.process(doc).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("503"), "error should contain HTTP status: {msg}");
    }

    #[tokio::test]
    async fn test_http_async_result_fetch_non_2xx_returns_error_with_status() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/convert/file/async"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-result-err", "task_status": "pending"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/status/poll/task-result-err"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-result-err", "task_status": "success"
            })))
            .mount(&server)
            .await;
        // Result fetch returns 404
        Mock::given(method("GET"))
            .and(path("/v1/result/task-result-err"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: None,
            timeout_secs: 10,
            use_async: true,
            poll_interval_ms: 50,
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        let err = p.process(doc).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("404"), "error should contain HTTP status: {msg}");
    }

    #[tokio::test]
    async fn test_http_async_unknown_task_status_returns_error_immediately() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/convert/file/async"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-unknown", "task_status": "pending"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/status/poll/task-unknown"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task_id": "task-unknown", "task_status": "cancelled"
            })))
            .mount(&server)
            .await;

        let p = DoclingPreprocessor::new(DoclingBackend::Http {
            base_url: server.uri(),
            api_key: None,
            timeout_secs: 10,
            use_async: true,
            poll_interval_ms: 50,
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        let err = p.process(doc).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("cancelled") || msg.contains("unexpected"),
            "error should name the unknown status: {msg}"
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cli_backend_error_on_missing_command() {
        let p = DoclingPreprocessor::new(DoclingBackend::Cli {
            command: "/nonexistent/docling".into(),
        });
        let doc = raw_doc(b"%PDF-1.4".to_vec(), "application/pdf");
        assert!(p.process(doc).await.is_err());
    }

    #[test]
    fn test_supported_mimes_and_mime_to_ext_are_consistent() {
        // Every MIME type in SUPPORTED_MIMES must have a real extension in mime_to_ext()
        // (not the fallback "bin"), which would mean it was missed.
        for mime in SUPPORTED_MIMES {
            let ext = mime_to_ext(mime);
            assert_ne!(
                ext, "bin",
                "MIME type {mime:?} is in SUPPORTED_MIMES but has no extension in mime_to_ext()"
            );
        }
    }

    #[test]
    fn canonical_evicts_after_read() {
        use serde_json::json;

        let pp = DoclingPreprocessor::new(crate::DoclingBackend::Cli { command: "echo".into() });
        let doc_id = DocumentId::new();
        let value = json!({"blocks": []});

        pp.set_canonical(&doc_id, value.clone());
        assert!(pp.canonical(&doc_id).is_some(), "first read should return value");
        assert!(pp.canonical(&doc_id).is_none(), "second read should return None — entry was evicted");
    }

    #[test]
    fn test_docling_chains_covers_all_supported_mimes() {
        use crate::preprocessors::registry::PreprocessorRegistry;

        struct NoOp;
        #[async_trait::async_trait]
        impl arcanum_core::traits::Preprocessor for NoOp {
            async fn process(&self, doc: arcanum_core::types::RawDocument) -> arcanum_core::Result<arcanum_core::types::RawDocument> {
                Ok(doc)
            }
            fn canonical(&self, _doc_id: &arcanum_core::types::DocumentId) -> Option<serde_json::Value> {
                None
            }
            fn set_canonical(&self, _doc_id: &arcanum_core::types::DocumentId, _canonical: serde_json::Value) {
            }
        }

        let registry = PreprocessorRegistry::docling_chains(std::sync::Arc::new(NoOp));
        let registered = registry.registered_mimes();

        for mime in SUPPORTED_MIMES {
            assert!(
                registered.contains(&mime.to_string()),
                "MIME {mime:?} is in SUPPORTED_MIMES but not registered in docling_chains()"
            );
        }
    }
}
