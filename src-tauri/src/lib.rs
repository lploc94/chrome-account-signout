pub mod workflow;
mod commands {
    use super::workflow::*;
    use std::sync::Arc;
    #[tauri::command]
    pub async fn list_accounts(
        engine: tauri::State<'_, Arc<Engine>>,
    ) -> std::result::Result<Vec<ProfileView>, String> {
        let e = engine.inner().clone();
        tauri::async_runtime::spawn_blocking(move || e.list().map_err(|e| e.to_string()))
            .await
            .map_err(|_| "Unable to load accounts.".to_string())?
    }
    #[tauri::command]
    pub fn prepare_logout(
        engine: tauri::State<'_, Arc<Engine>>,
        id: String,
    ) -> std::result::Result<Pending, String> {
        engine.prepare(&id).map_err(|e| e.to_string())
    }
    #[tauri::command]
    pub fn confirm_logout(
        engine: tauri::State<'_, Arc<Engine>>,
        token: String,
    ) -> std::result::Result<String, String> {
        engine.inner().confirm(&token).map_err(|e| e.to_string())
    }
    #[tauri::command]
    pub fn cancel_logout(
        engine: tauri::State<'_, Arc<Engine>>,
        id: String,
    ) -> std::result::Result<(), String> {
        engine.cancel(&id).map_err(|e| e.to_string())
    }
    #[tauri::command]
    pub fn operation_status(
        engine: tauri::State<'_, Arc<Engine>>,
        id: String,
    ) -> std::result::Result<Status, String> {
        engine.status(&id).map_err(|e| e.to_string())
    }
}
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::Manager;
use workflow::{Engine, NativeRuntime};
struct ExitWatch(AtomicBool);
fn defer_exit(app: tauri::AppHandle) -> bool {
    let engine = app.state::<Arc<Engine>>().inner().clone();
    if engine.request_exit() {
        return false;
    }
    if !app.state::<ExitWatch>().0.swap(true, Ordering::AcqRel) {
        tauri::async_runtime::spawn_blocking(move || {
            while engine.active() {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            app.exit(0);
        });
    }
    true
}
pub fn run() {
    let app = tauri::Builder::default()
        .manage(Engine::new(Arc::new(NativeRuntime)))
        .manage(ExitWatch(AtomicBool::new(false)))
        .setup(|app| {
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .on_navigation(|url| {
                    let local = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"));
                    let dev = cfg!(debug_assertions)
                        && url.scheme() == "http"
                        && url.host_str() == Some("127.0.0.1")
                        && url.port() == Some(1420);
                    local || dev
                })
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::prepare_logout,
            commands::confirm_logout,
            commands::cancel_logout,
            commands::operation_status
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && defer_exit(window.app_handle().clone())
            {
                api.prevent_close()
            }
        })
        .build(tauri::generate_context!())
        .expect("Could not open the application window");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event
            && defer_exit(app.clone())
        {
            api.prevent_exit()
        }
    });
}
