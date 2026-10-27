pub mod models;
pub mod providers;
pub mod rotation;
pub mod storage;
pub mod wallpaper;
mod commands;

use std::sync::Arc;
use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder, Emitter, Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let store = storage::Store::new(app.path().app_data_dir()?, app.path().app_cache_dir()?.join("media")).map_err(std::io::Error::other)?;
            let engine = Arc::new(commands::Engine::new(store).map_err(std::io::Error::other)?);
            commands::allow_local_assets(app.handle(), &engine.snapshot().map_err(std::io::Error::other)?).map_err(std::io::Error::other)?;
            app.manage(engine.clone());
            let show = MenuItem::with_id(app, "show", "Show Wallta", true, None::<&str>)?;
            let next = MenuItem::with_id(app, "next", "Next wallpaper", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "Pause / Resume", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Wallta", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &next, &pause, &quit])?;
            let mut tray = TrayIconBuilder::new().tooltip("Wallta — wallpaper rotation").menu(&menu).show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => if let Some(window) = app.get_webview_window("main") { let _ = window.unminimize(); let _ = window.show(); let _ = window.set_focus(); },
                    "quit" => app.exit(0),
                    "next" | "pause" => {
                        let engine = app.state::<Arc<commands::Engine>>().inner().clone();
                        let app = app.clone(); let next = event.id.as_ref() == "next";
                        std::thread::spawn(move || {
                            let result = if next { engine.rotate(false) } else { engine.snapshot().and_then(|s| engine.set_running(!s.running)) };
                            match result { Ok(state) => { let _ = app.emit("wallta:state", state); }, Err(error) => { let _ = app.emit("wallta:error", error); } }
                        });
                    }
                    _ => (),
                });
            if let Some(icon) = app.default_window_icon() { tray = tray.icon(icon.clone()); }
            tray.build(app)?;
            if std::env::args().any(|a| a == "--background") { if let Some(window) = app.get_webview_window("main") { window.hide()?; } }
            commands::start_worker(app.handle().clone(), engine);
            Ok(())
        })
        .on_window_event(|window, event| { if let WindowEvent::CloseRequested { api, .. } = event { api.prevent_close(); let _ = window.hide(); } })
        .invoke_handler(tauri::generate_handler![commands::get_state, commands::save_state, commands::pick_folder, commands::list_media, commands::next_wallpaper, commands::set_running, commands::get_capabilities, commands::clear_cache, commands::open_external])
        .run(tauri::generate_context!())
        .expect("Wallta failed to start");
}
