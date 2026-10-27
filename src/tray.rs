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
    state: edoproxy::proxy::AppState,
    port: u16,
    #[cfg(target_os = "macos")] startup_arguments: Vec<String>,
) -> anyhow::Result<()> {
    let image = image::load_from_memory(include_bytes!("../assets/icons/edoproxy_logo.png"))?
        .resize(32, 32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    #[cfg(target_os = "macos")]
    let image = {
        let mut image = image;
        for pixel in image.pixels_mut() {
            pixel.0[0] = 0;
            pixel.0[1] = 0;
            pixel.0[2] = 0;
        }
        image
    };
    let icon = tray_icon::Icon::from_rgba(image.clone().into_raw(), image.width(), image.height())?;
    let menu = Menu::new();
    let title = MenuItem::new(
        format!("edoproxy {}", env!("CARGO_PKG_VERSION")),
        false,
        None,
    );
    let local = MenuItem::new(format!("Local: http://127.0.0.1:{port}"), false, None);
    let target_label = || {
        state
            .target
            .read()
            .unwrap()
            .as_ref()
            .map(|url| format!("Target: {url}"))
            .unwrap_or_else(|| "Target: Not configured".to_owned())
    };
    let mut last_target = target_label();
    let target = MenuItem::new(&last_target, false, None);
    #[cfg(target_os = "macos")]
    let autostart = tray_icon::menu::CheckMenuItem::new(
        "Start at login",
        true,
        crate::autostart::enabled(),
        None,
    );
    #[cfg(target_os = "macos")]
    let startup_status = MenuItem::new("Startup changes apply at next login", false, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&title, &local, &target])?;
    #[cfg(target_os = "macos")]
    menu.append_items(&[&autostart, &startup_status])?;
    menu.append(&quit)?;
    let quit_id = quit.id().clone();
    let event_loop = EventLoopBuilder::<MenuEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    let event_loop = {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        let mut event_loop = event_loop;
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        event_loop.set_dock_visibility(false);
        event_loop.set_activate_ignoring_other_apps(false);
        event_loop
    };
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
            let builder = TrayIconBuilder::new()
                .with_tooltip(format!("edoproxy {}", env!("CARGO_PKG_VERSION")))
                .with_menu(Box::new(menu.take().unwrap()));
            #[cfg(target_os = "macos")]
            let builder = builder.with_icon_templated(icon.take().unwrap());
            #[cfg(not(target_os = "macos"))]
            let builder = builder.with_icon(icon.take().unwrap());
            match builder.build() {
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
        let current_target = state
            .target
            .read()
            .unwrap()
            .as_ref()
            .map(|url| format!("Target: {url}"))
            .unwrap_or_else(|| "Target: Not configured".to_owned());
        if current_target != last_target {
            target.set_text(&current_target);
            last_target = current_target;
        }
        #[cfg(target_os = "macos")]
        if let Event::UserEvent(ref event) = event
            && event.id == autostart.id()
        {
            if let Err(error) =
                crate::autostart::set_enabled(autostart.is_checked(), &startup_arguments)
            {
                tracing::error!(%error, "cannot change automatic startup");
                startup_status.set_text(format!("Startup: {error}"));
            } else {
                startup_status.set_text("Startup changes apply at next login");
            }
            autostart.set_checked(crate::autostart::enabled());
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
