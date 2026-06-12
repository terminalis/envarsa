// Envarsa — a local-first library for your environment values.
// Store-only librarian: it organizes, copies, and exports; it never
// writes into project trees and never injects into processes.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod crypto;
mod envfile;
mod state;
mod store;
mod update;

use state::AppState;
use tauri::Manager;

fn main() {
    let mut builder = tauri::Builder::default();
    // The selftest must be able to run while a normal Envarsa is open —
    // single-instance would silently forward to it and exit.
    if std::env::var("ENVARSA_SELFTEST").is_err() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let config_path = state::config_file_path(app.handle());
            let config = state::load_config(&config_path);
            let (store_path, env_override) = state::resolve_store_path(app.handle(), &config);
            let mut session = state::init_session(&store_path);
            state::maybe_seed_demo(&mut session, &store_path);
            *app.state::<AppState>().0.lock().unwrap() = Some(state::Inner {
                store_path,
                config_path,
                config,
                env_override,
                session,
                pending_import: None,
            });
            // No-op unless the user opted in (Settings → About).
            update::maybe_spawn_auto_check(app.handle().clone());
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                apply_brand_titlebar(&window);
            }
            Ok(())
        })
        // Dropped files are read here in Rust; the webview gets a
        // finished payload, never a path it could echo back.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                commands::handle_drop(window, paths);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::store_status,
            commands::unlock,
            commands::lock,
            commands::list_projects,
            commands::get_project,
            commands::preview_capture,
            commands::pick_env_file,
            commands::capture,
            commands::reveal_value,
            commands::copy_value,
            commands::copy_block,
            commands::export_snapshot,
            commands::export_to_path,
            commands::export_store,
            commands::export_store_to_path,
            commands::rename_project,
            commands::set_path_hint,
            commands::delete_project,
            commands::promote_snapshot,
            commands::enable_encryption,
            commands::change_passphrase,
            commands::disable_encryption,
            commands::reveal_store,
            commands::relocate_store,
            commands::check_for_updates,
            commands::set_auto_update_check,
            commands::open_releases_page,
            commands::pick_import_store,
            commands::inspect_import,
            commands::apply_import,
            commands::restore_backup,
            commands::ui_log,
            commands::selftest_enabled,
            commands::selftest_read_clipboard,
            commands::selftest_set_clipboard,
            commands::selftest_read_file,
            commands::selftest_stage_import,
            commands::selftest_done,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Envarsa");
}

// Windows 11 lets a window pick its own caption color; Windows 10 rejects
// these attributes with E_INVALIDARG and keeps the OS color — both fine,
// so the results are deliberately ignored.
#[cfg(windows)]
fn apply_brand_titlebar(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    // COLORREF is 0x00BBGGRR: brass #e5b35a caption, ink #221a09 text
    // (--accent / --accent-ink in ui/styles.css).
    let caption = COLORREF(0x005A_B3E5);
    let text = COLORREF(0x0009_1A22);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption as *const COLORREF as *const _,
            std::mem::size_of::<COLORREF>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &text as *const COLORREF as *const _,
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
}
