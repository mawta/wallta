mod commands;
pub mod google;
mod lan_commands;
pub mod models;
pub mod providers;
pub mod rotation;
pub mod storage;
mod update_policy;
mod updates;
pub mod wallpaper;

use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|a| a == "--background") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
        }))
        .setup(|app| {
            let store = storage::Store::new(
                app.path().app_data_dir()?,
                app.path().app_cache_dir()?.join("media"),
            )
            .map_err(std::io::Error::other)?;
            let engine = Arc::new(commands::Engine::new(store).map_err(std::io::Error::other)?);
            app.manage(engine.clone());
            app.manage(Arc::new(updates::UpdateManager::new(
                engine.store.root.clone(),
            )));
            app.manage(Arc::new(
                lan::LanManager::new(&engine.store.root).map_err(std::io::Error::other)?,
            ));
            let show = MenuItem::with_id(app, "show", "Show Wallta", true, None::<&str>)?;
            let next = MenuItem::with_id(app, "next", "Next wallpaper", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "Pause / Resume", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Wallta", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &next, &pause, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Wallta — wallpaper rotation")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.state::<Arc<lan::LanManager>>().stop();
                        app.exit(0);
                    }
                    "next" | "pause" => {
                        let engine = app.state::<Arc<commands::Engine>>().inner().clone();
                        let app = app.clone();
                        let next = event.id.as_ref() == "next";
                        std::thread::spawn(move || {
                            let result = if next {
                                engine.rotate(false)
                            } else {
                                engine
                                    .snapshot()
                                    .and_then(|s| engine.set_running(!s.running))
                            };
                            match result {
                                Ok(state) => {
                                    let _ = app.emit("wallta:state", state);
                                }
                                Err(error) => {
                                    let _ = app.emit("wallta:error", error);
                                }
                            }
                        });
                    }
                    _ => (),
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if std::env::args().any(|a| a == "--background") {
                if let Some(window) = app.get_webview_window("main") {
                    window.hide()?;
                }
            }
            commands::start_worker(app.handle().clone(), engine);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window.state::<Arc<lan::LanManager>>().stop();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_state,
            commands::pick_folder,
            commands::list_media,
            commands::get_preview,
            commands::next_wallpaper,
            commands::set_running,
            commands::get_capabilities,
            commands::clear_cache,
            commands::open_external,
            lan_commands::lan_start,
            lan_commands::lan_status,
            lan_commands::lan_stop,
            lan_commands::lan_accept,
            google::google_begin,
            google::google_poll,
            google::google_cancel,
            google::google_disconnect,
            updates::update_status,
            updates::update_check,
            updates::update_download,
            updates::update_cancel,
            updates::update_install
        ])
        .build(tauri::generate_context!())
        .expect("Wallta failed to start")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                app.state::<Arc<lan::LanManager>>().stop();
            }
        });
}

mod lan;
