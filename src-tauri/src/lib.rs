mod shortcuts;
mod storage;
mod tray;

use std::time::Duration;
use storage::{AppSettings, NoteDraft, NoteDraftMeta, NoteMeta, NoteRecord, Storage};
use tauri::{Emitter, Manager, State};

#[tauri::command]
fn get_settings(storage: State<'_, Storage>) -> Result<AppSettings, String> {
    storage.get_settings()
}

#[tauri::command]
fn set_notes_dir(storage: State<'_, Storage>, path: String) -> Result<AppSettings, String> {
    storage.set_notes_dir(path)
}

#[tauri::command]
fn set_shortcuts(
    app: tauri::AppHandle,
    storage: State<'_, Storage>,
    new_note: Option<String>,
    recent: Option<String>,
) -> Result<(), String> {
    let previous = storage.get_settings()?;
    storage.set_shortcuts(new_note.clone(), recent.clone())?;
    if let Err(error) = shortcuts::configure(&app, new_note, recent) {
        let _ = storage.set_shortcuts(
            previous.shortcut_new_note.clone(),
            previous.shortcut_recent_notes.clone(),
        );
        let _ = shortcuts::configure(
            &app,
            previous.shortcut_new_note,
            previous.shortcut_recent_notes,
        );
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
fn get_shortcut_status() -> shortcuts::ShortcutStatus {
    shortcuts::status()
}

#[tauri::command]
fn report_frontend_ready() {
    if std::env::var_os("QUICKNOTE_TRACE_WEBVIEW").is_some() {
        eprintln!("QUICKNOTE_FRONTEND_READY");
    }
}

#[tauri::command]
fn save_draft(storage: State<'_, Storage>, content: String) -> Result<(), String> {
    storage.save_draft(content)
}

#[tauri::command]
fn load_draft(storage: State<'_, Storage>) -> Result<String, String> {
    storage.load_draft()
}

#[tauri::command]
fn save_note(storage: State<'_, Storage>, content: String) -> Result<NoteMeta, String> {
    storage.save_note(content)
}

#[tauri::command]
fn list_notes(storage: State<'_, Storage>, query: Option<String>) -> Result<Vec<NoteMeta>, String> {
    storage.list_notes(query)
}

#[tauri::command]
fn get_recent_notes(storage: State<'_, Storage>) -> Result<Vec<NoteMeta>, String> {
    storage.get_recent_notes()
}

#[tauri::command]
fn read_note(storage: State<'_, Storage>, id: String) -> Result<NoteRecord, String> {
    storage.read_note(id)
}

#[tauri::command]
fn update_note(
    storage: State<'_, Storage>,
    id: String,
    content: String,
    expected_revision: String,
) -> Result<NoteMeta, String> {
    storage.update_note(id, content, expected_revision)
}

#[tauri::command]
fn delete_note(
    storage: State<'_, Storage>,
    id: String,
    expected_revision: String,
) -> Result<(), String> {
    storage.delete_note(id, expected_revision)
}

#[tauri::command]
fn get_note_draft(storage: State<'_, Storage>, id: String) -> Result<Option<NoteDraft>, String> {
    storage.get_note_draft(id)
}

#[tauri::command]
fn save_note_draft(
    storage: State<'_, Storage>,
    id: String,
    content: String,
    base_revision: Option<String>,
) -> Result<(), String> {
    storage.save_note_draft(id, content, base_revision)
}

#[tauri::command]
fn list_note_drafts(storage: State<'_, Storage>) -> Result<Vec<NoteDraftMeta>, String> {
    storage.list_note_drafts()
}

#[tauri::command]
fn discard_note_draft(storage: State<'_, Storage>, id: String) -> Result<(), String> {
    storage.discard_note_draft(id)
}

fn activate_window(app: &tauri::AppHandle, route: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let quick = route == "quick";
        let _ = window.set_always_on_top(quick);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let event = match route {
        "quick" => "open-quick-note",
        "recent" => "open-recent-notes",
        "archive" => "open-archive",
        "settings" => "open-settings",
        _ => return,
    };
    let _ = app.emit(event, ());
}

fn start_external_note_scan(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        let storage = app.state::<Storage>();
        if storage
            .get_settings()
            .map(|settings| settings.notes_dir.is_none())
            .unwrap_or(true)
        {
            continue;
        }
        match storage.scan_notes() {
            Ok(true) => {
                let _ = app.emit("notes-changed", ());
            }
            Ok(false) => {}
            Err(error) => log::warn!("notes folder scan failed: {error}"),
        }
    });
}

pub fn run() {
    if !cfg!(debug_assertions) && tauri::is_dev() {
        eprintln!(
            "Quicknote was built in release mode without the Tauri production asset protocol. Rebuild it with `npm run build:desktop`; do not launch a plain `cargo build --release` artifact."
        );
        return;
    }

    let builder = tauri::Builder::default()
        .on_page_load(|_, payload| {
            if std::env::var_os("QUICKNOTE_TRACE_WEBVIEW").is_some()
                && payload.event() == tauri::webview::PageLoadEvent::Finished
            {
                eprintln!("QUICKNOTE_WEBVIEW_FINISHED {}", payload.url());
            }
        })
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            activate_window(app, "quick")
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| shortcuts::on_shortcut(app, shortcut, event))
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_notes_dir,
            set_shortcuts,
            get_shortcut_status,
            report_frontend_ready,
            save_draft,
            load_draft,
            save_note,
            list_notes,
            get_recent_notes,
            read_note,
            update_note,
            delete_note,
            get_note_draft,
            save_note_draft,
            list_note_drafts,
            discard_note_draft
        ])
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let storage = Storage::new(config_dir, None).map_err(std::io::Error::other)?;
            app.manage(storage);
            start_external_note_scan(app.handle().clone());

            let settings = app
                .state::<Storage>()
                .get_settings()
                .map_err(std::io::Error::other)?;
            shortcuts::start(
                app.handle().clone(),
                settings.shortcut_new_note,
                settings.shortcut_recent_notes,
            );
            tray::install(app.handle())?;

            if let Some(window) = app.get_webview_window("main") {
                let window_to_hide = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_to_hide.hide();
                    }
                });
            }
            activate_window(app.handle(), "archive");
            Ok(())
        });

    builder
        .run(tauri::generate_context!())
        .expect("failed to run Quicknote");
}
