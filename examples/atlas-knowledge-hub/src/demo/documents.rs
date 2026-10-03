use super::{require_key, DemoCtx, DemoError};
use arcanum_core::types::DocumentId;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct DocumentText {
    pub document_id: String,
    pub version_num: u32,
    pub source_uri: String,
    pub status: String,
    pub mime_type: String,
    pub text: String,
}

/// `GET /demo/documents/:document_id/versions/:n/text`: the exact string that evidence byte
/// offsets (`offset_start..offset_end`) index.
///
/// The pipeline registers chunks against `String::from_utf8_lossy(doc.content)` of the
/// preprocessed document. The canonical sidecar is only the Docling `blocks` JSON and holds no
/// text, and Atlas registers a pass-through preprocessor, so the stored raw snapshot is that same content.
/// Invalid UTF-8 is an error rather than lossily decoded, because lossy decoding would shift
/// the offsets.
pub async fn document_text(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
    Path((document_id, n)): Path<(String, u32)>,
) -> Result<Json<DocumentText>, DemoError> {
    require_key(&ctx.state, &headers)?;
    let uuid = uuid::Uuid::parse_str(&document_id)
        .map_err(|_| DemoError::BadRequest(format!("malformed document id: {document_id}")))?;
    let id = DocumentId(uuid);
    let engine = &ctx.state.engine;
    let version = engine
        .version_store
        .get_version(&id, n)
        .await
        .map_err(|e| DemoError::Internal(e.to_string()))?
        .ok_or_else(|| DemoError::NotFound(format!("no version {n} of document {document_id}")))?;
    let raw = engine
        .snapshot_store
        .fetch_raw(&version.snapshot_uri)
        .await
        .map_err(|e| DemoError::Internal(format!("snapshot unavailable: {e}")))?;
    let text = String::from_utf8(raw)
        .map_err(|_| DemoError::Internal("snapshot is not valid UTF-8".into()))?;
    Ok(Json(DocumentText {
        document_id,
        version_num: version.version_num,
        source_uri: version.source_uri,
        status: format!("{:?}", version.status),
        mime_type: version.mime_type,
        text,
    }))
}
