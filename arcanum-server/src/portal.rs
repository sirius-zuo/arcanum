const PORTAL_HTML: &[u8] = include_bytes!("../assets/admin/index.html");

pub async fn serve_portal() -> axum::response::Html<String> {
    axum::response::Html(String::from_utf8_lossy(PORTAL_HTML).to_string())
}
