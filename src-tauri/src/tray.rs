use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("Quicknote must have a generated default application icon");
    let quick = MenuItem::with_id(app, "new-note", "Quick note", true, None::<&str>)?;
    let recent = MenuItem::with_id(app, "recent-notes", "All notes", true, None::<&str>)?;
    let archive = MenuItem::with_id(app, "archive", "Notes library", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&quick, &recent, &archive, &separator, &settings, &quit],
    )?;

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Quicknote")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "new-note" => crate::activate_window(app, "quick"),
            "recent-notes" => crate::activate_window(app, "recent"),
            "archive" => crate::activate_window(app, "archive"),
            "settings" => crate::activate_window(app, "settings"),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
