// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use serde::Deserialize;
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use url::Url;

#[derive(Clone)]
pub struct AppState {
    pub target: Arc<RwLock<Option<Url>>>,
    origin: HeaderValue,
    client: reqwest::Client,
}
impl AppState {
    pub fn new(origin: &str) -> anyhow::Result<Self> {
        crate::config::validate_origin(origin)?;
        Ok(Self {
            target: Default::default(),
            origin: origin.parse()?,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .build()?,
        })
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/target", post(set_target))
        .route("/proxy", any(proxy))
        .route("/proxy/", any(proxy))
        .route("/proxy/{*path}", any(proxy))
        .fallback(|| async { error(StatusCode::NOT_FOUND, "unknown endpoint") })
        .layer(middleware::from_fn_with_state(state.clone(), origin_guard))
        .with_state(state)
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

async fn origin_guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    // Reject before handlers execute: CORS response headers alone do not prevent writes.
    let origins = request.headers().get_all(header::ORIGIN);
    let allowed = origins.iter().count() == 1 && origins.iter().next() == Some(&state.origin);
    // Origin-less health checks support service monitoring; all other calls require the SPA origin.
    if !(allowed
        || (origins.iter().next().is_none()
            && request.method() == Method::GET
            && request.uri().path() == "/health"))
    {
        return error(StatusCode::FORBIDDEN, "origin not allowed");
    }
    let preflight_headers = request
        .headers()
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .cloned();
    let mut response = if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    if allowed {
        let headers = response.headers_mut();
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, state.origin);
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS"),
        );
        if let Some(value) = preflight_headers {
            headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, value);
        }
        headers.append(
            header::VARY,
            HeaderValue::from_static("Origin, Access-Control-Request-Headers"),
        );
    }
    response
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"status":"ok", "version":env!("CARGO_PKG_VERSION"), "target_configured":state.target.read().unwrap().is_some()}),
    )
}
#[derive(Deserialize)]
struct Target {
    url: String,
}
async fn set_target(State(state): State<AppState>, Json(input): Json<Target>) -> Response {
    let Ok(url) = Url::parse(&input.url) else {
        return error(StatusCode::BAD_REQUEST, "invalid target URL");
    };
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return error(
            StatusCode::BAD_REQUEST,
            "target must be an HTTP(S) origin without credentials, path, query or fragment",
        );
    }
    *state.target.write().unwrap() = Some(url);
    StatusCode::NO_CONTENT.into_response()
}

fn clean_headers(headers: &mut HeaderMap) {
    let named: Vec<String> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|v| v.trim().to_owned())
        .collect();
    for name in named {
        headers.remove(name);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "host",
    ] {
        headers.remove(name);
    }
}

async fn proxy(State(state): State<AppState>, request: Request) -> Response {
    let Some(target) = state.target.read().unwrap().clone() else {
        return error(StatusCode::CONFLICT, "no target configured");
    };
    let (mut parts, body) = request.into_parts();
    let suffix = parts.uri.path().strip_prefix("/proxy").unwrap_or("/");
    let path = if suffix.is_empty() { "/" } else { suffix };
    // Append to the fixed authority; never URL-join a user-controlled //host path.
    let destination = format!(
        "{}{}{}",
        target.as_str().trim_end_matches('/'),
        path,
        parts
            .uri
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default()
    );
    clean_headers(&mut parts.headers);
    parts.headers.remove(header::ORIGIN);
    parts.headers.remove(header::REFERER);
    let upstream = state
        .client
        .request(parts.method, destination)
        .headers(parts.headers)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await;
    match upstream {
        Ok(upstream) => {
            let status = upstream.status();
            let mut headers = upstream.headers().clone();
            clean_headers(&mut headers);
            let cors: Vec<_> = headers
                .keys()
                .filter(|k| k.as_str().starts_with("access-control-"))
                .cloned()
                .collect();
            for name in cors {
                headers.remove(name);
            }
            let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            response
        }
        Err(err) => {
            tracing::warn!(timeout = err.is_timeout(), "upstream request failed");
            error(
                if err.is_timeout() {
                    StatusCode::GATEWAY_TIMEOUT
                } else {
                    StatusCode::BAD_GATEWAY
                },
                "upstream request failed",
            )
        }
    }
}
