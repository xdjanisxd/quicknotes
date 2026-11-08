#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--quicknote-check-build") {
        println!(
            "{}",
            if tauri::is_dev() {
                "development"
            } else {
                "production"
            }
        );
        return;
    }
    quicknote_lib::run();
}
