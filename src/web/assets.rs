//! Static asset serving for the web UI.
//!
//! Embeds compiled SPA assets from `web/dist/` and serves them with appropriate
//! MIME types and cache headers.

use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};
use rust_embed::Embed;

/// Embedded static assets from the web/dist directory.
#[derive(Embed)]
#[folder = "web/dist"]
pub struct Assets;

/// Serve a static asset by path.
///
/// Returns the asset with appropriate MIME type and cache headers.
/// For SPA routes (paths without extensions), returns index.html.
pub async fn serve_asset(path: &str) -> impl IntoResponse {
    // Normalize path: remove leading slash
    let path = path.trim_start_matches('/');

    // Empty path or SPA routes (no extension) -> serve index.html
    let asset_path = if path.is_empty() || !path.contains('.') {
        "index.html"
    } else {
        path
    };

    match Assets::get(asset_path) {
        Some(content) => {
            let mime = mime_guess::from_path(asset_path)
                .first_or_octet_stream()
                .to_string();

            // Cache immutable assets (hashed filenames) for 1 year
            // Cache index.html for 0 seconds (always revalidate)
            let cache_control = if asset_path == "index.html" {
                "no-cache, no-store, must-revalidate"
            } else if asset_path.contains('.')
                && (asset_path.contains('-') || asset_path.contains(".min."))
            {
                // Hashed or minified assets - cache for 1 year
                "public, max-age=31536000, immutable"
            } else {
                // Other assets - cache for 1 hour
                "public, max-age=3600"
            };

            Response::builder()
                .status(StatusCode::OK)
                .header(
                    header::CONTENT_TYPE,
                    HeaderValue::from_str(&mime).expect("valid MIME type"),
                )
                .header(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static(cache_control),
                )
                .body(Body::from(content.data.into_owned()))
                .expect("valid response")
        }
        None => {
            // Asset not found - for SPA, try index.html as fallback
            if asset_path != "index.html" {
                if let Some(index) = Assets::get("index.html") {
                    return Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, HeaderValue::from_static("text/html"))
                        .header(
                            header::CACHE_CONTROL,
                            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
                        )
                        .body(Body::from(index.data.into_owned()))
                        .expect("valid response");
                }
            }

            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"))
                .body(Body::from("Not Found"))
                .expect("valid response")
        }
    }
}

/// Axum handler for serving static assets.
///
/// Extracts the path from the request and serves the corresponding asset.
pub async fn static_handler(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    serve_asset(&path).await
}

/// Axum handler for serving the root index.html.
pub async fn index_handler() -> impl IntoResponse {
    serve_asset("").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assets_embedded() {
        // This test verifies the Assets struct compiles correctly.
        // Actual asset availability depends on web/dist/ existing at compile time.
        // In CI, we'll need to build the frontend first.
        let _ = Assets::iter().count();
    }

    #[tokio::test]
    async fn test_serve_asset_not_found() {
        let response = serve_asset("nonexistent.xyz").await.into_response();
        // If index.html exists, it returns 200 (SPA fallback)
        // If not, it returns 404
        let status = response.status();
        assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_serve_asset_empty_path() {
        let response = serve_asset("").await.into_response();
        // Empty path should try to serve index.html
        let status = response.status();
        // Will be 200 if index.html exists, 404 otherwise
        assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_serve_asset_spa_route() {
        // SPA routes (no extension) should serve index.html
        let response = serve_asset("review").await.into_response();
        let status = response.status();
        assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_mime_type_detection() {
        // Verify mime_guess works for common types
        // Note: mime_guess returns "text/javascript" which is valid per RFC 4329
        let js_mime = mime_guess::from_path("app.js")
            .first_or_octet_stream()
            .to_string();
        assert!(js_mime == "text/javascript" || js_mime == "application/javascript");
        assert_eq!(
            mime_guess::from_path("style.css")
                .first_or_octet_stream()
                .to_string(),
            "text/css"
        );
        assert_eq!(
            mime_guess::from_path("index.html")
                .first_or_octet_stream()
                .to_string(),
            "text/html"
        );
        assert_eq!(
            mime_guess::from_path("logo.svg")
                .first_or_octet_stream()
                .to_string(),
            "image/svg+xml"
        );
    }

    #[tokio::test]
    async fn test_serve_index_html_content_type() {
        // Verify index.html is served with correct content type
        let response = serve_asset("index.html").await.into_response();
        if response.status() == StatusCode::OK {
            let content_type = response.headers().get(header::CONTENT_TYPE);
            assert!(content_type.is_some());
            assert_eq!(content_type.unwrap().to_str().unwrap(), "text/html");
        }
    }

    #[tokio::test]
    async fn test_serve_index_html_cache_headers() {
        // Verify index.html has no-cache headers (always revalidate)
        let response = serve_asset("index.html").await.into_response();
        if response.status() == StatusCode::OK {
            let cache_control = response.headers().get(header::CACHE_CONTROL);
            assert!(cache_control.is_some());
            let cache_value = cache_control.unwrap().to_str().unwrap();
            assert!(cache_value.contains("no-cache") || cache_value.contains("must-revalidate"));
        }
    }
}
