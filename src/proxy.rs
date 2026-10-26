// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get},
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
    origins: Arc<RwLock<Vec<HeaderValue>>>,
    port: Arc<RwLock<u16>>,
    active_port: u16,
    client: reqwest::Client,
    config_path: Option<std::path::PathBuf>,
    update_lock: Arc<std::sync::Mutex<()>>,
}
impl AppState {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Arc::new(RwLock::new(port));
        self.active_port = port;
        self
    }

    pub fn with_config_path(mut self, path: std::path::PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    pub fn new(origins: &[String], target: Option<&str>) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !origins.is_empty(),
            "at least one allowed origin is required"
        );
        for origin in origins {
            crate::config::validate_origin(origin)?;
        }
        let target = target.map(crate::config::validate_target).transpose()?;
        Ok(Self {
            config_path: None,
            update_lock: Default::default(),
            target: Arc::new(RwLock::new(target)),
            port: Arc::new(RwLock::new(8765)),
            active_port: 8765,
            origins: Arc::new(RwLock::new(
                origins
                    .iter()
                    .map(|origin| origin.parse())
                    .collect::<Result<_, _>>()?,
            )),
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
        .route("/edoproxy/health", get(health))
        .route("/edoproxy/target", get(get_target).post(set_target))
        .route("/edoproxy/origin", get(get_origins).post(set_origins))
        .route("/edoproxy/port", get(get_port).post(set_port))
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
    let matched_origin = origins
        .iter()
        .next()
        .filter(|origin| {
            origins.iter().count() == 1 && state.origins.read().unwrap().contains(origin)
        })
        .cloned();
    let allowed = matched_origin.is_some();
    // Origin-less health checks support service monitoring; all other calls require the SPA origin.
    if !(allowed
        || (origins.iter().next().is_none()
            && request.method() == Method::GET
            && request.uri().path() == "/edoproxy/health"))
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
    if let Some(origin) = matched_origin {
        let headers = response.headers_mut();
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
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
    let Ok(url) = crate::config::validate_target(&input.url) else {
        return error(
            StatusCode::BAD_REQUEST,
            "target must be an HTTP(S) origin without credentials, path, query or fragment",
        );
    };
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        // Serialize disk and memory updates so concurrent requests cannot diverge.
        let _update = state.update_lock.lock().unwrap();
        if let Some(path) = &state.config_path {
            crate::config::save_value(path, "target", toml_edit::value(url.as_str()))?;
        }
        *state.target.write().unwrap() = Some(url);
        Ok(())
    })
    .await;
    match result {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        result => {
            tracing::error!(?result, "failed to persist target");
            error(StatusCode::INTERNAL_SERVER_ERROR, "could not save target")
        }
    }
}

async fn get_target(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"url": state.target.read().unwrap().as_ref().map(Url::as_str)}))
}

async fn get_origins(State(state): State<AppState>) -> Json<serde_json::Value> {
    let origins: Vec<String> = state
        .origins
        .read()
        .unwrap()
        .iter()
        .map(|origin| origin.to_str().unwrap().to_owned())
        .collect();
    Json(serde_json::json!({"origins": origins}))
}

async fn get_port(State(state): State<AppState>) -> Json<serde_json::Value> {
    let port = *state.port.read().unwrap();
    Json(
        serde_json::json!({"port": port, "active_port": state.active_port, "restart_required": port != state.active_port}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Origins {
    origins: Vec<String>,
}

async fn set_origins(State(state): State<AppState>, Json(input): Json<Origins>) -> Response {
    if input.origins.is_empty()
        || input
            .origins
            .iter()
            .any(|origin| crate::config::validate_origin(origin).is_err())
    {
        return error(
            StatusCode::BAD_REQUEST,
            "origins must be a non-empty list of exact HTTP(S) origins",
        );
    }
    let headers: Vec<HeaderValue> =
        match input.origins.iter().map(|origin| origin.parse()).collect() {
            Ok(headers) => headers,
            Err(_) => return error(StatusCode::BAD_REQUEST, "invalid origin header"),
        };
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let _update = state.update_lock.lock().unwrap();
        if let Some(path) = &state.config_path {
            let mut array = toml_edit::Array::new();
            for origin in input.origins {
                array.push(origin);
            }
            crate::config::save_value(path, "origins", toml_edit::value(array))?;
        }
        *state.origins.write().unwrap() = headers;
        Ok(())
    })
    .await;
    match result {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        result => {
            tracing::error!(?result, "failed to persist origins");
            error(StatusCode::INTERNAL_SERVER_ERROR, "could not save origins")
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Port {
    port: u16,
}

async fn set_port(State(state): State<AppState>, Json(input): Json<Port>) -> Response {
    if input.port == 0 {
        return error(StatusCode::BAD_REQUEST, "port must be between 1 and 65535");
    }
    let response_state = state.clone();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let _update = state.update_lock.lock().unwrap();
        if let Some(path) = &state.config_path {
            crate::config::save_value(path, "port", toml_edit::value(i64::from(input.port)))?;
        }
        *state.port.write().unwrap() = input.port;
        Ok(())
    })
    .await;
    match result {
        Ok(Ok(())) => get_port(State(response_state)).await.into_response(),
        result => {
            tracing::error!(?result, "failed to persist port");
            error(StatusCode::INTERNAL_SERVER_ERROR, "could not save port")
        }
    }
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
