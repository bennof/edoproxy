// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

pub fn run(
    stop: tokio::sync::oneshot::Sender<()>,
    runtime: tokio::runtime::Runtime,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
) -> anyhow::Result<()> {
    let image = image::load_from_memory(include_bytes!("../assets/icons/edoproxy_logo.png"))?
        .resize(32, 32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let icon = tray_icon::Icon::from_rgba(image.clone().into_raw(), image.width(), image.height())?;
    let menu = Menu::new();
    let title = MenuItem::new("edoproxy", false, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&title, &quit])?;
    let quit_id = quit.id().clone();
    let event_loop = EventLoopBuilder::<MenuEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(event);
    }));
    let mut tray = None;
    let mut icon = Some(icon);
    let mut menu = Some(menu);
    let mut stop = Some(stop);
    let mut server = Some(server);
    event_loop.run(move |event, _, control| {
        *control = ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_millis(250),
        );
        if let Event::NewEvents(StartCause::Init) = event {
            match TrayIconBuilder::new()
                .with_tooltip("edoproxy")
                .with_icon(icon.take().unwrap())
                .with_menu(Box::new(menu.take().unwrap()))
                .build()
            {
                Ok(value) => tray = Some(value),
                Err(err) => {
                    tracing::error!(%err, "tray creation failed");
                    *control = ControlFlow::Exit;
                }
            }
        }
        if let Event::UserEvent(ref event) = event
            && event.id == quit_id
        {
            *control = ControlFlow::Exit;
        }
        if server.as_ref().is_some_and(|server| server.is_finished()) {
            *control = ControlFlow::Exit;
        }
        if let Event::LoopDestroyed = event {
            tray.take();
            if let Some(stop) = stop.take() {
                let _ = stop.send(());
            }
            if let Some(server) = server.take() {
                match runtime.block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(5), server).await
                }) {
                    Ok(Ok(Ok(()))) => {}
                    result => tracing::error!(?result, "server shutdown incomplete"),
                }
            }
        }
    });
}
