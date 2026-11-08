use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const STATE_FILE: &str = "notes-state.json";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppSettings {
    pub notes_dir: Option<String>,
    pub shortcut_new_note: Option<String>,
    pub shortcut_recent_notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteMeta {
    /// Slash-separated path relative to the selected notes directory.
    pub id: String,
    /// Compatibility alias used by the UI for displaying the vault-relative path.
    pub path: String,
    pub title: String,
    pub preview: String,
    pub updated_at: String,
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteRecord {
    pub meta: NoteMeta,
    pub content: String,
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteDraftMeta {
    pub id: String,
    pub updated_at: String,
    #[serde(default)]
    pub base_revision: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteDraft {
    pub content: String,
    pub updated_at: String,
    pub base_revision: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PersistedState {
    settings: AppSettings,
    quick_draft: String,
    note_drafts: BTreeMap<String, StoredDraft>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredDraft {
    content: String,
    updated_at: String,
    #[serde(default)]
    base_revision: Option<String>,
}

/// Filesystem-backed Markdown vault and app-private settings/drafts.
///
/// Each instance is intended to be managed by Tauri as application state. All vault IDs are
/// relative Markdown paths, never arbitrary filesystem paths.
pub struct Storage {
    state_path: PathBuf,
    state_lock: Mutex<()>,
    vault_root: Mutex<Option<PathBuf>>,
    last_scan_fingerprint: Mutex<Option<String>>,
}

impl Storage {
    pub fn new(
        app_config_dir: PathBuf,
        default_notes_dir: Option<PathBuf>,
    ) -> Result<Self, String> {
        fs::create_dir_all(&app_config_dir).map_err(err("create app config directory"))?;
        let canonical_config =
            fs::canonicalize(&app_config_dir).map_err(err("resolve app config directory"))?;
        let state_path = canonical_config.join(STATE_FILE);
        let persisted = read_state(&state_path)?;

        let configured_dir = persisted.settings.notes_dir.as_deref().map(PathBuf::from);
        let requested_dir = configured_dir.or(default_notes_dir);
        let vault_root = match requested_dir {
            Some(path) => Some(canonical_vault(&path)?),
            None => None,
        };
        let storage = Self {
            state_path,
            state_lock: Mutex::new(()),
            vault_root: Mutex::new(vault_root),
            last_scan_fingerprint: Mutex::new(None),
        };
        if let Some(root) = storage.root_optional()? {
            let mut state = read_state(&storage.state_path)?;
            state.settings.notes_dir = Some(root.to_string_lossy().into_owned());
            storage.write_state(&state)?;
            *storage
                .last_scan_fingerprint
                .lock()
                .map_err(|_| "scan state lock poisoned".to_string())? =
                Some(storage.fingerprint(&root)?);
        }
        Ok(storage)
    }

    pub fn get_settings(&self) -> Result<AppSettings, String> {
        Ok(read_state(&self.state_path)?.settings)
    }

    pub fn set_notes_dir(&self, path: String) -> Result<AppSettings, String> {
        let root = canonical_vault(Path::new(&path))?;
        let fingerprint = self.fingerprint(&root)?;
        let _state_guard = self.state_guard()?;
        let mut state = read_state(&self.state_path)?;
        state.settings.notes_dir = Some(root.to_string_lossy().into_owned());
        self.write_state(&state)?;
        *self
            .vault_root
            .lock()
            .map_err(|_| "notes folder lock poisoned".to_string())? = Some(root.clone());
        *self
            .last_scan_fingerprint
            .lock()
            .map_err(|_| "scan state lock poisoned".to_string())? = Some(fingerprint);
        Ok(state.settings)
    }

    pub fn set_shortcuts(
        &self,
        shortcut_new_note: Option<String>,
        shortcut_recent_notes: Option<String>,
    ) -> Result<AppSettings, String> {
        let _state_guard = self.state_guard()?;
        let mut state = read_state(&self.state_path)?;
        state.settings.shortcut_new_note = shortcut_new_note;
        state.settings.shortcut_recent_notes = shortcut_recent_notes;
        self.write_state(&state)?;
        Ok(state.settings)
    }

    pub fn save_draft(&self, content: String) -> Result<(), String> {
        let _state_guard = self.state_guard()?;
        let mut state = read_state(&self.state_path)?;
        state.quick_draft = content;
        self.write_state(&state)
    }

    pub fn load_draft(&self) -> Result<String, String> {
        Ok(read_state(&self.state_path)?.quick_draft)
    }

    pub fn save_note(&self, content: String) -> Result<NoteMeta, String> {
        let root = self.root()?;
        let (id, target) = self.unique_note_path(&root)?;
        atomic_create(&target, content.as_bytes())?;
        let meta = self.meta_for(&root, &id, &content)?;
        let _state_guard = self.state_guard()?;
        let mut state = read_state(&self.state_path)?;
        state.quick_draft.clear();
        self.write_state(&state)?;
        Ok(meta)
    }

    pub fn list_notes(&self, query: Option<String>) -> Result<Vec<NoteMeta>, String> {
        let root = self.root()?;
        let mut notes = Vec::new();
        self.collect_notes(&root, &root, &mut notes)?;
        if let Some(query) = query.filter(|query| !query.trim().is_empty()) {
            let query = query.to_lowercase();
            notes.retain(|meta| {
                if meta.title.to_lowercase().contains(&query)
                    || meta.preview.to_lowercase().contains(&query)
                {
                    return true;
                }
                let Ok((_, path)) = self.note_path(&root, &meta.id) else {
                    return false;
                };
                fs::read_to_string(path)
                    .map(|content| content.to_lowercase().contains(&query))
                    .unwrap_or(false)
            });
        }
        notes.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(notes)
    }

    pub fn get_recent_notes(&self) -> Result<Vec<NoteMeta>, String> {
        self.list_notes(None)
    }

    pub fn read_note(&self, id: String) -> Result<NoteRecord, String> {
        let root = self.root()?;
        let (_path, canonical) = self.note_path(&root, &id)?;
        let bytes = fs::read(&canonical).map_err(err("read note"))?;
        let content =
            String::from_utf8(bytes).map_err(|_| "note is not valid UTF-8".to_string())?;
        let meta = self.meta_for(&root, &id, &content)?;
        Ok(NoteRecord {
            revision: meta.revision.clone(),
            meta,
            content,
        })
    }

    pub fn update_note(
        &self,
        id: String,
        content: String,
        expected_revision: String,
    ) -> Result<NoteMeta, String> {
        let root = self.root()?;
        let (_path, canonical) = self.note_path(&root, &id)?;
        let current = fs::read(&canonical).map_err(err("read note before update"))?;
        let current_text =
            String::from_utf8(current).map_err(|_| "note is not valid UTF-8".to_string())?;
        if revision(current_text.as_bytes()) != expected_revision {
            return Err("note changed outside the app; reload before saving".into());
        }
        // Replace through the canonical path resolved under the vault. This avoids following
        // a parent directory that was swapped for a symlink after note_path validated it.
        atomic_replace(&canonical, content.as_bytes())?;
        self.meta_for(&root, &id, &content)
    }

    pub fn delete_note(&self, id: String, expected_revision: String) -> Result<(), String> {
        let root = self.root()?;
        let (_path, canonical) = self.note_path(&root, &id)?;
        let current = fs::read(&canonical).map_err(err("read note before delete"))?;
        if revision(&current) != expected_revision {
            return Err("note changed outside the app; reload before deleting".into());
        }
        fs::remove_file(canonical).map_err(err("delete note"))
    }

    pub fn get_note_draft(&self, id: String) -> Result<Option<NoteDraft>, String> {
        validate_id(&id)?;
        Ok(read_state(&self.state_path)?
            .note_drafts
            .get(&id)
            .map(|draft| NoteDraft {
                content: draft.content.clone(),
                updated_at: draft.updated_at.clone(),
                base_revision: draft.base_revision.clone(),
            }))
    }

    pub fn save_note_draft(
        &self,
        id: String,
        content: String,
        base_revision: Option<String>,
    ) -> Result<(), String> {
        validate_id(&id)?;
        let _state_guard = self.state_guard()?;
        let mut state = read_state(&self.state_path)?;
        state.note_drafts.insert(
            id,
            StoredDraft {
                content,
                updated_at: now_millis(),
                base_revision,
            },
        );
        self.write_state(&state)
    }

    pub fn list_note_drafts(&self) -> Result<Vec<NoteDraftMeta>, String> {
        let state = read_state(&self.state_path)?;
        let mut drafts: Vec<_> = state
            .note_drafts
            .into_iter()
            .map(|(id, draft)| NoteDraftMeta {
                id,
                updated_at: draft.updated_at,
                base_revision: draft.base_revision,
            })
            .collect();
        drafts.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(drafts)
    }

    pub fn discard_note_draft(&self, id: String) -> Result<(), String> {
        validate_id(&id)?;
        let _state_guard = self.state_guard()?;
        let mut state = read_state(&self.state_path)?;
        state.note_drafts.remove(&id);
        self.write_state(&state)
    }

    /// Returns true when the set or contents of visible Markdown files changed since the
    /// previous call. Hidden directories and symlinks are excluded from the fingerprint.
    pub fn scan_notes(&self) -> Result<bool, String> {
        let root = self.root()?;
        let fingerprint = self.fingerprint(&root)?;
        let mut last = self
            .last_scan_fingerprint
            .lock()
            .map_err(|_| "scan state lock poisoned".to_string())?;
        let changed = last
            .as_ref()
            .map(|old| old != &fingerprint)
            .unwrap_or(false);
        *last = Some(fingerprint);
        Ok(changed)
    }

    fn root_optional(&self) -> Result<Option<PathBuf>, String> {
        Ok(self
            .vault_root
            .lock()
            .map_err(|_| "notes folder lock poisoned".to_string())?
            .clone())
    }

    fn state_guard(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.state_lock
            .lock()
            .map_err(|_| "app state lock poisoned".to_string())
    }

    fn root(&self) -> Result<PathBuf, String> {
        let root = self
            .root_optional()?
            .ok_or_else(|| "notes folder is not configured".to_string())?;
        let current = fs::canonicalize(&root).map_err(err("resolve notes folder"))?;
        if current != root || !current.is_dir() {
            return Err("notes folder is no longer the selected directory".into());
        }
        Ok(root)
    }

    fn write_state(&self, state: &PersistedState) -> Result<(), String> {
        let data =
            serde_json::to_vec_pretty(state).map_err(|e| format!("serialize app state: {e}"))?;
        atomic_replace(&self.state_path, &data)
    }

    fn unique_note_path(&self, root: &Path) -> Result<(String, PathBuf), String> {
        let (stamp, _) = utc_filename_stamp();
        for attempt in 0..1000u32 {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!("-{attempt}")
            };
            let id = format!("{stamp}{suffix}.md");
            let path = root.join(&id);
            if !path.exists() {
                return Ok((id, path));
            }
        }
        Err("could not allocate a unique note filename".into())
    }

    fn note_path(&self, root: &Path, id: &str) -> Result<(PathBuf, PathBuf), String> {
        validate_id(id)?;
        let path = root.join(id);
        let mut ancestor = root.to_path_buf();
        let components: Vec<_> = Path::new(id).components().collect();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if let Component::Normal(part) = component {
                ancestor.push(part);
                let metadata =
                    fs::symlink_metadata(&ancestor).map_err(err("find note directory"))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("note path contains an unsafe directory".into());
                }
            }
        }
        let metadata = fs::symlink_metadata(&path).map_err(err("find note"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("note path is not a regular file".into());
        }
        let canonical = fs::canonicalize(&path).map_err(err("resolve note path"))?;
        if !canonical.starts_with(root) {
            return Err("note path escapes the selected folder".into());
        }
        Ok((path, canonical))
    }

    fn meta_for(&self, root: &Path, id: &str, content: &str) -> Result<NoteMeta, String> {
        let (_, canonical) = self.note_path(root, id)?;
        let modified = fs::metadata(canonical)
            .and_then(|m| m.modified())
            .map_err(err("read note timestamp"))?;
        Ok(NoteMeta {
            id: id.to_string(),
            path: id.to_string(),
            title: title_for(content),
            preview: preview_for(content),
            updated_at: system_time_millis(modified),
            revision: revision(content.as_bytes()),
        })
    }

    fn collect_notes(
        &self,
        root: &Path,
        dir: &Path,
        notes: &mut Vec<NoteMeta>,
    ) -> Result<(), String> {
        let metadata = match fs::symlink_metadata(dir) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("inspect notes directory: {error}")),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Ok(());
        }
        let canonical_dir = fs::canonicalize(dir).map_err(err("resolve notes directory"))?;
        if !canonical_dir.starts_with(root) {
            return Ok(());
        }
        let entries = fs::read_dir(dir).map_err(err("scan notes folder"))?;
        for entry in entries {
            let entry = entry.map_err(err("read folder entry"))?;
            let name = entry.file_name();
            let name_string = name.to_string_lossy();
            if name_string.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(err("inspect folder entry"))?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                self.collect_notes(root, &path, notes)?;
            } else if metadata.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                let canonical = fs::canonicalize(&path).map_err(err("resolve note path"))?;
                if !canonical.starts_with(root) {
                    continue;
                }
                let bytes = fs::read(&canonical).map_err(err("read note"))?;
                let content = String::from_utf8(bytes)
                    .map_err(|_| format!("note is not valid UTF-8: {}", path.display()))?;
                let id = path
                    .strip_prefix(root)
                    .map_err(|error| format!("make note ID: {error}"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                notes.push(self.meta_for(root, &id, &content)?);
            }
        }
        Ok(())
    }

    fn fingerprint(&self, root: &Path) -> Result<String, String> {
        let mut notes = Vec::new();
        self.collect_notes(root, root, &mut notes)?;
        notes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut bytes = Vec::new();
        for note in notes {
            bytes.extend_from_slice(note.id.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(note.revision.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(note.updated_at.as_bytes());
            bytes.push(0xff);
        }
        Ok(revision(&bytes))
    }
}

fn read_state(path: &Path) -> Result<PersistedState, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("read app state: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PersistedState::default()),
        Err(e) => Err(format!("read app state: {e}")),
    }
}

fn canonical_vault(path: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(err("resolve notes folder"))?;
    if !canonical.is_dir() {
        return Err("notes folder is not a directory".into());
    }
    Ok(canonical)
}

fn validate_id(id: &str) -> Result<(), String> {
    let path = Path::new(id);
    if id.is_empty() || path.is_absolute() || !id.to_ascii_lowercase().ends_with(".md") {
        return Err("invalid note ID".into());
    }
    for component in path.components() {
        match component {
            Component::Normal(part) if !part.to_string_lossy().starts_with('.') => {}
            _ => return Err("invalid note ID".into()),
        }
    }
    Ok(())
}

fn title_for(content: &str) -> String {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled")
        .to_string()
}

fn preview_for(content: &str) -> String {
    let compact = content.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(240).collect()
}

fn revision(bytes: &[u8]) -> String {
    // FNV-1a is sufficient as a change token; it is not used for security or authentication.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn now_millis() -> String {
    system_time_millis(SystemTime::now())
}

fn system_time_millis(time: SystemTime) -> String {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn utc_filename_stamp() -> (String, u128) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let secs = (millis / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let day_secs = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;
    let fraction = millis % 1000;
    let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    (
        format!(
            "{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}-{fraction:03}-{serial}"
        ),
        millis,
    )
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

fn atomic_create(path: &Path, data: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "note path has no parent directory".to_string())?;
    let (temp, mut file) = create_temp(parent)?;
    let result = (|| {
        file.write_all(data).map_err(err("write new note"))?;
        file.sync_all().map_err(err("sync new note"))?;
        fs::hard_link(&temp, path).map_err(err("publish new note"))?;
        fs::remove_file(&temp).map_err(err("remove temporary note"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn atomic_replace(path: &Path, data: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "file path has no parent directory".to_string())?;
    let (temp, mut file) = create_temp(parent)?;
    let result = (|| {
        file.write_all(data).map_err(err("write temporary file"))?;
        file.sync_all().map_err(err("sync temporary file"))?;
        fs::rename(&temp, path).map_err(err("replace file"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn create_temp(parent: &Path) -> Result<(PathBuf, File), String> {
    for _ in 0..100 {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".notes-tmp-{}-{serial}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("create temporary file: {error}")),
        }
    }
    Err("could not allocate a temporary file".into())
}

fn err(context: &'static str) -> impl FnOnce(std::io::Error) -> String {
    move |error| format!("{context}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, TempDir, Storage) {
        let app = tempfile::tempdir().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let store =
            Storage::new(app.path().to_path_buf(), Some(vault.path().to_path_buf())).unwrap();
        (app, vault, store)
    }

    #[test]
    fn create_update_delete_and_drafts_persist() {
        let (app, _vault, store) = setup();
        store.save_draft("quick draft".into()).unwrap();
        let meta = store.save_note("\nA title\nbody".into()).unwrap();
        assert_eq!(meta.title, "A title");
        assert_eq!(store.load_draft().unwrap(), "");
        assert_eq!(
            store.read_note(meta.id.clone()).unwrap().content,
            "\nA title\nbody"
        );
        store
            .save_note_draft(
                meta.id.clone(),
                "unsaved edit".into(),
                Some(meta.revision.clone()),
            )
            .unwrap();
        let reopened = Storage::new(app.path().to_path_buf(), None).unwrap();
        assert_eq!(
            reopened
                .get_note_draft(meta.id.clone())
                .unwrap()
                .unwrap()
                .content,
            "unsaved edit"
        );
        let changed = store
            .update_note(meta.id.clone(), "Changed".into(), meta.revision.clone())
            .unwrap();
        assert_eq!(store.read_note(meta.id.clone()).unwrap().content, "Changed");
        store.delete_note(meta.id, changed.revision).unwrap();
        assert!(store.list_notes(None).unwrap().is_empty());
    }

    #[test]
    fn external_edit_is_a_conflict_for_update_and_delete() {
        let (_app, vault, store) = setup();
        let meta = store.save_note("original".into()).unwrap();
        fs::write(vault.path().join(&meta.id), "outside edit").unwrap();
        assert!(store
            .update_note(meta.id.clone(), "mine".into(), meta.revision.clone())
            .unwrap_err()
            .contains("changed outside"));
        assert!(store
            .delete_note(meta.id, meta.revision)
            .unwrap_err()
            .contains("changed outside"));
    }

    #[test]
    fn scans_visible_markdown_only_and_detects_external_changes() {
        let (_app, vault, store) = setup();
        fs::create_dir(vault.path().join("sub")).unwrap();
        fs::write(vault.path().join("sub/a.md"), "alpha").unwrap();
        fs::create_dir(vault.path().join(".hidden")).unwrap();
        fs::write(vault.path().join(".hidden/b.md"), "hidden").unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::write(external.path().join("escape.md"), "outside").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(external.path(), vault.path().join("linked")).unwrap();
        let visible = store.list_notes(None).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "sub/a.md");
        assert!(store.scan_notes().unwrap());
        assert!(!store.scan_notes().unwrap());
        fs::write(vault.path().join("sub/a.md"), "edited").unwrap();
        assert!(store.scan_notes().unwrap());
        assert!(store.read_note("linked/escape.md".into()).is_err());
    }

    #[test]
    fn list_sorts_modified_desc_and_searches_content() {
        let (_app, vault, store) = setup();
        fs::write(vault.path().join("a.md"), "First\nneedle in text").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(vault.path().join("b.md"), "Second").unwrap();
        let all = store.list_notes(None).unwrap();
        assert_eq!(all[0].id, "b.md");
        assert_eq!(
            store.list_notes(Some("needle".into())).unwrap()[0].id,
            "a.md"
        );
    }

    #[test]
    fn recent_notes_returns_all_notes_newest_first() {
        let (_app, _vault, store) = setup();
        let mut created = Vec::new();
        for index in 0..7 {
            created.push(store.save_note(format!("Note {index}")).unwrap().id);
            if index < 6 {
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
        }

        let recent = store.get_recent_notes().unwrap();
        assert_eq!(recent.len(), 7);
        assert_eq!(recent.first().unwrap().id, created[6]);
        assert_eq!(recent.last().unwrap().id, created[0]);
    }
}
