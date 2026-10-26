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
    router(AppState::new(&[ORIGIN.to_owned()], None).unwrap())
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
        .oneshot(request("GET", "/edoproxy/health", None, ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    for origin in [None, Some("https://evil.example.org"), Some("null")] {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/edoproxy/target",
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
        ("POST", "/edoproxy/target", StatusCode::BAD_REQUEST),
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
                "/edoproxy/target",
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
            "/edoproxy/target",
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

#[tokio::test]
async fn multiple_origins_and_initial_target() {
    let state = AppState::new(
        &[ORIGIN.to_owned(), "https://apps.falkner.xyz".to_owned()],
        Some("https://server.example.org"),
    )
    .unwrap();
    let app = router(state.clone());
    for origin in [ORIGIN, "https://apps.falkner.xyz"] {
        let response = app
            .clone()
            .oneshot(request("OPTIONS", "/edoproxy/target", Some(origin), ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(response.headers()["access-control-allow-origin"], origin);
    }
    let response = app
        .clone()
        .oneshot(request("GET", "/edoproxy/health", None, ""))
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["target_configured"], true);
    let mut duplicate = request(
        "POST",
        "/edoproxy/target",
        Some(ORIGIN),
        r#"{"url":"https://other.example.org"}"#,
    );
    duplicate
        .headers_mut()
        .append("origin", ORIGIN.parse().unwrap());
    assert_eq!(
        app.clone().oneshot(duplicate).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let response = app
        .oneshot(request(
            "POST",
            "/edoproxy/target",
            Some(ORIGIN),
            r#"{"url":"https://other.example.org"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        state.target.read().unwrap().as_ref().unwrap().host_str(),
        Some("other.example.org")
    );
}

#[tokio::test]
async fn target_is_persisted_and_failed_save_keeps_current_target() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let initial = "# Keep this comment\norigins = ['https://spa.example.org']\nport = 9876\n";
    std::fs::write(&path, initial).unwrap();
    let state = AppState::new(&[ORIGIN.to_owned()], None)
        .unwrap()
        .with_config_path(path.clone());
    let app = router(state.clone());
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/edoproxy/target",
            Some(ORIGIN),
            r#"{"url":"https://saved.example.org"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# Keep this comment"));
    let restored = edoproxy::config::Settings::from_toml(&text).unwrap();
    assert_eq!(
        restored.target.as_deref(),
        Some("https://saved.example.org/")
    );
    assert_eq!(restored.port, 9876);
    assert_eq!(restored.origins, [ORIGIN]);
    // Deterministic write failure on every platform, even with elevated permissions.
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let response = app
        .oneshot(request(
            "POST",
            "/edoproxy/target",
            Some(ORIGIN),
            r#"{"url":"https://not-saved.example.org"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.headers()["access-control-allow-origin"], ORIGIN);
    assert_eq!(
        state.target.read().unwrap().as_ref().unwrap().as_str(),
        "https://saved.example.org/"
    );
}

#[tokio::test]
async fn management_endpoints_persist_and_apply_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, format!("origins = ['{ORIGIN}']\nport = 8765\n")).unwrap();
    let app = router(
        AppState::new(&[ORIGIN.to_owned()], None)
            .unwrap()
            .with_config_path(path.clone()),
    );
    for endpoint in ["target", "origin", "port"] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/edoproxy/{endpoint}"),
                    Some(ORIGIN),
                    ""
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(request("POST", "/edoproxy/health", Some(ORIGIN), "{}"))
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        app.clone()
            .oneshot(request("GET", "/health", Some(ORIGIN), ""))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/edoproxy/port",
            Some(ORIGIN),
            r#"{"port":9000}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"port":9000,"active_port":8765,"restart_required":true})
    );
    for (endpoint, body) in [
        ("port", r#"{"port":0}"#),
        ("origin", r#"{"origins":[]}"#),
        ("origin", r#"{"origins":["*"]}"#),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    &format!("/edoproxy/{endpoint}"),
                    Some(ORIGIN),
                    body
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/edoproxy/origin",
            Some(ORIGIN),
            r#"{"origins":["https://new.example.org"]}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(response.headers()["access-control-allow-origin"], ORIGIN);
    assert_eq!(
        app.clone()
            .oneshot(request("GET", "/edoproxy/origin", Some(ORIGIN), ""))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                "/edoproxy/origin",
                Some("https://new.example.org"),
                ""
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let settings =
        edoproxy::config::Settings::from_toml(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(settings.port, 9000);
    assert_eq!(settings.origins, ["https://new.example.org"]);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    for (endpoint, body) in [
        ("port", r#"{"port":9100}"#),
        ("origin", r#"{"origins":["https://failed.example.org"]}"#),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    &format!("/edoproxy/{endpoint}"),
                    Some("https://new.example.org"),
                    body
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    let response = app
        .oneshot(request(
            "GET",
            "/edoproxy/port",
            Some("https://new.example.org"),
            "",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body["port"], 9000);
}
