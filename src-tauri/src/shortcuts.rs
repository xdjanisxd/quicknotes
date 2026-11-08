use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};
use tauri::Emitter;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

#[derive(Clone, Debug, Serialize)]
pub struct ShortcutStatus {
    pub available: bool,
    pub message: Option<String>,
}

#[derive(Clone, Debug)]
struct ShortcutConfig {
    new_note: Option<String>,
    recent: Option<String>,
}

static STATUS: OnceLock<Mutex<ShortcutStatus>> = OnceLock::new();
static ROUTES: OnceLock<Mutex<HashMap<u32, &'static str>>> = OnceLock::new();

fn status_slot() -> &'static Mutex<ShortcutStatus> {
    STATUS.get_or_init(|| {
        Mutex::new(ShortcutStatus {
            available: false,
            message: None,
        })
    })
}

fn set_status(app: &tauri::AppHandle, available: bool, message: Option<String>) {
    let value = ShortcutStatus { available, message };
    if let Ok(mut status) = status_slot().lock() {
        *status = value.clone();
    }
    let _ = app.emit("shortcut-status", value);
}

pub fn status() -> ShortcutStatus {
    status_slot()
        .lock()
        .map(|status| status.clone())
        .unwrap_or(ShortcutStatus {
            available: false,
            message: Some("Global shortcut status is unavailable.".into()),
        })
}

pub fn on_shortcut(app: &tauri::AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let route = ROUTES
        .get()
        .and_then(|routes| routes.lock().ok())
        .and_then(|routes| routes.get(&shortcut.id()).copied());
    if let Some(route) = route {
        crate::activate_window(app, route);
    }
}

pub fn start(app: tauri::AppHandle, new_note: Option<String>, recent: Option<String>) {
    let config = ShortcutConfig { new_note, recent };
    #[cfg(target_os = "linux")]
    if is_wayland() {
        wayland::start(app.clone(), app.config().identifier.clone(), config);
        return;
    }
    if let Err(error) = configure_native(&app, config) {
        set_status(&app, false, Some(error));
    }
}

pub fn configure(
    app: &tauri::AppHandle,
    new_note: Option<String>,
    recent: Option<String>,
) -> Result<(), String> {
    let config = ShortcutConfig { new_note, recent };
    #[cfg(target_os = "linux")]
    if is_wayland() {
        return wayland::reconfigure(config);
    }
    configure_native(app, config)
}

fn parse_shortcut(value: &str) -> Result<Shortcut, String> {
    #[cfg(target_os = "macos")]
    let normalized = value
        .split('+')
        .map(|part| {
            if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
                "CmdOrControl"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("+");
    #[cfg(not(target_os = "macos"))]
    let normalized = value.to_string();
    Shortcut::from_str(&normalized).map_err(|error| format!("invalid shortcut '{value}': {error}"))
}

fn configure_native(app: &tauri::AppHandle, config: ShortcutConfig) -> Result<(), String> {
    let new_shortcut = config.new_note.as_deref().map(parse_shortcut).transpose()?;
    let recent_shortcut = config.recent.as_deref().map(parse_shortcut).transpose()?;
    if new_shortcut
        .as_ref()
        .zip(recent_shortcut.as_ref())
        .is_some_and(|(a, b)| a.id() == b.id())
    {
        return Err("Both actions cannot use the same shortcut.".into());
    }

    let manager = app.global_shortcut();
    manager
        .unregister_all()
        .map_err(|error| format!("could not clear previous shortcuts: {error}"))?;
    let mut routes = HashMap::new();
    if let Some(shortcut) = new_shortcut {
        manager
            .register(shortcut)
            .map_err(|error| format!("could not register new-note shortcut: {error}"))?;
        routes.insert(shortcut.id(), "quick");
    }
    if let Some(shortcut) = recent_shortcut {
        if let Err(error) = manager.register(shortcut) {
            let _ = manager.unregister_all();
            return Err(format!("could not register recent-notes shortcut: {error}"));
        }
        routes.insert(shortcut.id(), "recent");
    }
    let slot = ROUTES.get_or_init(|| Mutex::new(HashMap::new()));
    *slot
        .lock()
        .map_err(|_| "shortcut state lock is poisoned".to_string())? = routes;
    set_status(app, true, None);
    Ok(())
}

#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var("XDG_SESSION_TYPE")
            .map(|value| value.eq_ignore_ascii_case("wayland"))
            .unwrap_or(true)
}

#[cfg(not(target_os = "linux"))]
fn is_wayland() -> bool {
    false
}

#[cfg(target_os = "linux")]
mod wayland {
    use super::{set_status, ShortcutConfig};
    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
    use ashpd::desktop::CreateSessionOptions;
    use futures_util::{FutureExt, StreamExt};
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    static TX: OnceLock<async_channel::Sender<ShortcutConfig>> = OnceLock::new();

    pub fn start(app: tauri::AppHandle, app_id: String, config: ShortcutConfig) {
        let (tx, rx) = async_channel::unbounded();
        let _ = TX.set(tx);
        std::thread::spawn(move || {
            async_std::task::block_on(async move {
                let mut config = config;
                let mut identity_registered = false;
                loop {
                    match bind_and_listen(
                        &app,
                        &app_id,
                        &mut identity_registered,
                        config.clone(),
                        &rx,
                    )
                    .await
                    {
                        Ok(next) => config = next,
                        Err(error) => {
                            log::warn!("Wayland global shortcut portal unavailable: {error}");
                            set_status(&app, false, Some(format!("Wayland global shortcuts are unavailable: {error}. Use the tray menu instead.")));
                            match rx.recv().await {
                                Ok(next) => config = next,
                                Err(_) => break,
                            }
                        }
                    }
                }
            });
        });
    }

    pub fn reconfigure(config: ShortcutConfig) -> Result<(), String> {
        let Some(tx) = TX.get() else {
            return Err("Wayland shortcut portal has not started yet.".into());
        };
        tx.try_send(config)
            .map_err(|error| format!("could not update Wayland shortcuts: {error}"))
    }

    async fn bind_and_listen(
        app: &tauri::AppHandle,
        app_id: &str,
        identity_registered: &mut bool,
        config: ShortcutConfig,
        rx: &async_channel::Receiver<ShortcutConfig>,
    ) -> Result<ShortcutConfig, String> {
        if !*identity_registered {
            let parsed_app_id = app_id
                .parse::<ashpd::AppID>()
                .map_err(|error| format!("invalid application id '{app_id}': {error}"))?;
            ensure_desktop_entry(app_id)?;
            ashpd::register_host_app(parsed_app_id)
                .await
                .map_err(|error| {
                    format!("could not register application id with desktop portal: {error}")
                })?;
            *identity_registered = true;
        }

        let portal = GlobalShortcuts::new()
            .await
            .map_err(|error| error.to_string())?;
        let session = portal
            .create_session(CreateSessionOptions::default())
            .await
            .map_err(|error| error.to_string())?;
        let mut shortcuts = Vec::new();
        if let Some(trigger) = config.new_note.as_deref() {
            let preferred_trigger = portal_trigger(trigger)?;
            shortcuts.push(
                NewShortcut::new("new-note", "Open a new Quicknote")
                    .preferred_trigger(Some(preferred_trigger.as_str())),
            );
        }
        if let Some(trigger) = config.recent.as_deref() {
            let preferred_trigger = portal_trigger(trigger)?;
            shortcuts.push(
                NewShortcut::new("recent-notes", "Show all notes, most recently edited first")
                    .preferred_trigger(Some(preferred_trigger.as_str())),
            );
        }

        if !shortcuts.is_empty() {
            let request = portal
                .bind_shortcuts(&session, &shortcuts, None, BindShortcutsOptions::default())
                .await
                .map_err(|error| error.to_string())?;
            let bound = request.response().map_err(|error| error.to_string())?;
            if bound.shortcuts().is_empty() {
                return Err("the desktop did not accept any configured shortcuts".into());
            }
        }

        let mut activated = portal
            .receive_activated()
            .await
            .map_err(|error| error.to_string())?;
        set_status(app, true, None);
        loop {
            futures_util::select! {
                event = activated.next().fuse() => {
                    let Some(event) = event else { return Err("the desktop portal signal stream ended".into()); };
                    match event.shortcut_id() {
                        "new-note" => crate::activate_window(app, "quick"),
                        "recent-notes" => crate::activate_window(app, "recent"),
                        _ => {}
                    }
                },
                update = rx.recv().fuse() => {
                    let next = update.map_err(|_| "shortcut configuration channel closed".to_string())?;
                    let _ = session.close().await;
                    return Ok(next);
                }
            }
        }
    }

    fn ensure_desktop_entry(app_id: &str) -> Result<(), String> {
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .ok_or_else(|| "could not locate the user's XDG data directory".to_string())?;
        let executable = std::env::current_exe()
            .map_err(|error| format!("could not resolve the application executable: {error}"))?;
        ensure_desktop_entry_at(&data_home, app_id, &executable)
    }

    fn ensure_desktop_entry_at(
        data_home: &std::path::Path,
        app_id: &str,
        executable: &std::path::Path,
    ) -> Result<(), String> {
        let applications = data_home.join("applications");
        fs::create_dir_all(&applications).map_err(|error| {
            format!("could not create the user applications directory: {error}")
        })?;
        let desktop_file = applications.join(format!("{app_id}.desktop"));
        let escaped_executable = executable
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%");
        let contents = format!(
            "[Desktop Entry]\nType=Application\nName=Quicknote\nComment=Fast local notes\nExec=\"{escaped_executable}\"\nTerminal=false\nCategories=Utility;\nStartupWMClass={app_id}\n"
        );
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&desktop_file)
        {
            Ok(mut file) => file
                .write_all(contents.as_bytes())
                .map_err(|error| format!("could not write the application desktop entry: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(format!(
                "could not create the application desktop entry: {error}"
            )),
        }
    }

    fn portal_trigger(value: &str) -> Result<String, String> {
        let parts: Vec<_> = value
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            return Err("shortcut is empty".into());
        }
        let (modifiers, key) = parts.split_at(parts.len() - 1);
        let mut result = String::new();
        for modifier in modifiers {
            let tag = match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => "Control",
                "alt" | "option" => "Alt",
                "shift" => "Shift",
                "super" | "meta" | "cmd" | "command" | "win" | "windows" => "Super",
                other => return Err(format!("unsupported shortcut modifier: {other}")),
            };
            result.push('<');
            result.push_str(tag);
            result.push('>');
        }
        let key = match key[0].to_ascii_lowercase().as_str() {
            "space" => "space".to_string(),
            "enter" | "return" => "Return".to_string(),
            "esc" | "escape" => "Escape".to_string(),
            "tab" => "Tab".to_string(),
            "backspace" => "BackSpace".to_string(),
            "up" | "arrowup" => "Up".to_string(),
            "down" | "arrowdown" => "Down".to_string(),
            "left" | "arrowleft" => "Left".to_string(),
            "right" | "arrowright" => "Right".to_string(),
            raw if raw.len() == 1 => raw.to_string(),
            raw if raw.starts_with('f') && raw[1..].parse::<u8>().is_ok() => raw.to_string(),
            other => return Err(format!("unsupported shortcut key: {other}")),
        };
        result.push_str(&key);
        Ok(result)
    }

    #[cfg(test)]
    mod tests {
        use super::ensure_desktop_entry_at;
        use std::fs;
        use std::path::Path;

        #[test]
        fn creates_matching_desktop_id_with_escaped_executable_and_preserves_existing_entry() {
            let temp = tempfile::tempdir().unwrap();
            let executable = Path::new("/tmp/Quicknote % \"test\"");

            ensure_desktop_entry_at(temp.path(), "app.quicknote.local", executable).unwrap();

            let entry = temp.path().join("applications/app.quicknote.local.desktop");
            let contents = fs::read_to_string(&entry).unwrap();
            assert!(contents.contains("Exec=\"/tmp/Quicknote %% \\\"test\\\"\""));
            assert!(contents.contains("StartupWMClass=app.quicknote.local"));

            fs::write(&entry, "keep this desktop entry").unwrap();
            ensure_desktop_entry_at(temp.path(), "app.quicknote.local", executable).unwrap();
            assert_eq!(
                fs::read_to_string(entry).unwrap(),
                "keep this desktop entry"
            );
        }
    }
}
