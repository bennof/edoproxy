// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use clap::Parser;
use edoproxy::{
    config::Config,
    proxy::{AppState, router},
};
#[cfg(target_os = "macos")]
mod autostart;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod tray;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "edoproxy=info".into()),
        )
        .init();
    let config = Config::parse();
    let settings = config.load()?;
    let state = AppState::new(&settings.origins, settings.target.as_deref())?
        .with_config_path(config.path()?)
        .with_port(settings.port);
    let runtime = tokio::runtime::Runtime::new()?;
    let listener = runtime.block_on(tokio::net::TcpListener::bind((
        std::net::Ipv4Addr::LOCALHOST,
        settings.port,
    )))?;
    tracing::info!(address = %listener.local_addr()?, "proxy listening");
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let tray_state = state.clone();
    #[cfg(target_os = "macos")]
    let startup_arguments = {
        let mut args = vec![
            "--config".to_owned(),
            config.path()?.to_string_lossy().into_owned(),
        ];
        for origin in &config.origin {
            args.extend(["--origin".to_owned(), origin.clone()]);
        }
        if let Some(port) = config.port {
            args.extend(["--port".to_owned(), port.to_string()]);
        }
        args
    };
    let server = runtime.spawn(async move {
        axum::serve(listener, router(state))
            .with_graceful_shutdown(async move {
                tokio::select! { _ = stop_rx => {}, _ = shutdown_signal() => {} }
            })
            .await
    });
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    if !config.headless {
        return tray::run(
            stop_tx,
            runtime,
            server,
            tray_state,
            settings.port,
            #[cfg(target_os = "macos")]
            startup_arguments,
        );
    }
    let _keep_shutdown_sender = stop_tx;
    runtime.block_on(server)??;
    Ok(())
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
