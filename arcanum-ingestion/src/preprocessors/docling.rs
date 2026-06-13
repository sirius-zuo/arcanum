use std::time::{Duration, Instant};

use arcanum_core::{traits::Preprocessor, types::RawDocument, ArcanumError, Result};
use async_trait::async_trait;

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
    backend: DoclingBackend,
    client: reqwest::Client,
}

impl DoclingPreprocessor {
    pub fn new(backend: DoclingBackend) -> Self {
        Self {
            backend,
            client: reqwest::Client::new(),
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

        let file_part = reqwest::multipart::Part::bytes(doc.content.clone())
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

        let mut req = self
            .client
            .post(&endpoint)
            .timeout(Duration::from_secs(timeout_secs))
            .multipart(form);

        if let Some(key) = api_key {
            req = req.header("X-Api-Key", key.as_str());
        }

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
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
            let md = Self::extract_md(resp).await?;
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
            if Instant::now() > deadline {
                return Err(ArcanumError::Ingestion(format!(
                    "docling async conversion timed out (task {task_id})"
                )));
            }

            tokio::time::sleep(Duration::from_millis(poll_interval_ms)).await;

            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut poll_req = self
                .client
                .get(format!("{base_url}/v1/status/poll/{task_id}"))
                .timeout(remaining);
            if let Some(key) = api_key {
                poll_req = poll_req.header("X-Api-Key", key.as_str());
            }
            let poll: PollResponse = poll_req
                .send()
                .await
                .map_err(|e| ArcanumError::Ingestion(format!("docling poll request failed: {e}")))?
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
                _ => { /* pending / started — keep polling */ }
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

        let md = Self::extract_md(result_resp).await?;
        Ok(RawDocument {
            content: md.into_bytes(),
            mime_type: "text/markdown".to_string(),
            ..doc
        })
    }

    async fn extract_md(resp: reqwest::Response) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct ConvertResponse {
            document: ConvertedDoc,
        }
        #[derive(serde::Deserialize)]
        struct ConvertedDoc {
            md_content: Option<String>,
        }

        let body: ConvertResponse = resp
            .json()
            .await
            .map_err(|e| ArcanumError::Ingestion(format!("docling response parse error: {e}")))?;

        body.document
            .md_content
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ArcanumError::Ingestion(
                    "docling returned empty or missing md_content".into(),
                )
            })
    }

    async fn convert_via_cli(&self, doc: RawDocument, _command: &str) -> Result<RawDocument> {
        Err(ArcanumError::Ingestion(
            "DoclingPreprocessor CLI backend not yet implemented".into(),
        ))
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
}
