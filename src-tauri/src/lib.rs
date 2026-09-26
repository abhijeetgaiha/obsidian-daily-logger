mod commands;
mod config;
mod draft;
mod error;

use commands::AppState;
use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("config.json");
            app.manage(AppState::new(path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_draft,
            commands::load_settings,
            commands::set_fallback,
            commands::read_settings_form,
            commands::pick_vault_folder,
            commands::save_settings,
            commands::submit_entry,
            commands::request_exit,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle();
                if let Err(error) = commands::exit(app, &app.state::<AppState>()) {
                    eprintln!("{}: {}", error.code, error.message);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building the application")
        .run(|app, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                let result = commands::lock_session(&state.session)
                    .and_then(|mut session| session.prepare_exit(&state.config_path, None));
                if let Err(error) = result {
                    api.prevent_exit();
                    eprintln!("{}: {}", error.code, error.message);
                }
            }
        });
}
