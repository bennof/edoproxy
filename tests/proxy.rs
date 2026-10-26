// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::any,
};
use edoproxy::proxy::{AppState, router};
use tower::ServiceExt;
const ORIGIN: &str = "https://spa.example.org";
fn app() -> Router {
    router(AppState::new(ORIGIN).unwrap())
}
fn request(method: &str, path: &str, origin: Option<&str>, body: &str) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(origin) = origin {
        req = req.header("origin", origin);
    }
    req.body(Body::from(body.to_owned())).unwrap()
}
#[tokio::test]
async fn health_and_origin_enforcement() {
    let app = app();
    let response = app
        .clone()
        .oneshot(request("GET", "/health", None, ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    for origin in [None, Some("https://evil.example.org"), Some("null")] {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/target",
                origin,
                r#"{"url":"https://server.example.org"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-origin")
        );
    }
    let response = app
        .oneshot(request("GET", "/proxy/test", Some(ORIGIN), ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(response.headers()["access-control-allow-origin"], ORIGIN);
}
#[tokio::test]
async fn preflight_and_errors_have_cors() {
    for (method, path, expected) in [
        ("OPTIONS", "/proxy/test", StatusCode::NO_CONTENT),
        ("GET", "/missing", StatusCode::NOT_FOUND),
        ("POST", "/target", StatusCode::BAD_REQUEST),
    ] {
        let mut request = request(method, path, Some(ORIGIN), "bad json");
        request.headers_mut().insert(
            "access-control-request-headers",
            "authorization, content-type".parse().unwrap(),
        );
        let response = app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(response.headers()["access-control-allow-origin"], ORIGIN);
    }
}
#[tokio::test]
async fn rejects_non_origin_targets() {
    for target in [
        "file:///etc/passwd",
        "https://user:pass@example.org",
        "https://example.org/path",
        "https://example.org?x=1",
    ] {
        let response = app()
            .oneshot(request(
                "POST",
                "/target",
                Some(ORIGIN),
                &serde_json::json!({"url":target}).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
#[tokio::test]
async fn forwards_method_path_query_body_and_authorization() {
    let upstream = Router::new().fallback(any(|request: Request<Body>| async move {
        assert_eq!(request.method(), "PATCH");
        assert_eq!(request.uri(), "/api/schema?x=a%20b");
        assert_eq!(request.headers()["authorization"], "Bearer test");
        assert!(!request.headers().contains_key("x-hop"));
        assert!(!request.headers().contains_key("origin"));
        assert_eq!(
            to_bytes(request.into_body(), 1024).await.unwrap(),
            "payload"
        );
        (
            StatusCode::CREATED,
            [("access-control-allow-origin", "*")],
            "reply",
        )
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, upstream).await.unwrap();
    });
    let app = app();
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/target",
            Some(ORIGIN),
            &serde_json::json!({"url":format!("http://{address}")}).to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let mut req = request(
        "PATCH",
        "/proxy/api/schema?x=a%20b",
        Some(ORIGIN),
        "payload",
    );
    req.headers_mut()
        .insert("authorization", "Bearer test".parse().unwrap());
    req.headers_mut()
        .insert("connection", "x-hop".parse().unwrap());
    req.headers_mut().insert("x-hop", "remove".parse().unwrap());
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["access-control-allow-origin"], ORIGIN);
    assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), "reply");
    server.abort();
}
