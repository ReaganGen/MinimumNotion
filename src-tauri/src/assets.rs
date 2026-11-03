//! The `forma-asset://localhost/<page-id>/<reference>` protocol, which serves
//! images and attachments to the webview. Every request is resolved by the
//! Document Service, which refuses anything outside the workspace
//! (ARCHITECTURE §6.6).

use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext};

use crate::session::Session;

pub const SCHEME: &str = "forma-asset";

fn content_type(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .body(Vec::new())
        .expect("a static response is valid")
}

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let path = request.uri().path().trim_start_matches('/');
    let Some((page_id, reference)) = path.split_once('/') else {
        return status(StatusCode::BAD_REQUEST);
    };
    let session = ctx.app_handle().state::<Session>();
    let resolved = match session.read(|ws| ws.resolve_asset(page_id, reference)) {
        Ok(path) => path,
        Err(err) => {
            log::debug!("asset request refused: {}", err.message);
            return status(StatusCode::NOT_FOUND);
        }
    };
    match std::fs::read(&resolved) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type(&resolved))
            .header("X-Content-Type-Options", "nosniff")
            // SVGs are images here: no scripts, no outside requests.
            .header(
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'",
            )
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(err) => {
            log::warn!("couldn't read attachment {resolved:?}: {err}");
            status(StatusCode::NOT_FOUND)
        }
    }
}
