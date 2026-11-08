import { confirm, open } from '@tauri-apps/plugin-dialog';
import { api, errorMessage, setCompactWindow, type AppSettings, type NoteMeta, type NoteRecord } from './api';
import { chooseTheme, currentTheme, themes } from './theme';
import './style.css';

type Route = 'archive' | 'quick' | 'recent' | 'settings';
const app = document.querySelector<HTMLDivElement>('#app')!;
let route: Route = (new URLSearchParams(location.search).get('mode') as Route) || 'archive';
let notes: NoteMeta[] = [];
let draftIds = new Set<string>();
let settings: AppSettings = { notes_dir: null, shortcut_new_note: null, shortcut_recent_notes: null };
let active: NoteRecord | null = null;
let editText = '';
let quickText = '';
let selectedRecent = 0;
let recentDeletePending = false;
let recentEditing: NoteRecord | null = null;
let recentEditText = '';
let searchText = '';
let busy = false;
let externalConflict = false;
let shortcutStatus: { available: boolean; message: string | null } = { available: true, message: null };
let quickTimer: number | undefined;
let noteDraftTimer: number | undefined;
let toastTimer: number | undefined;

function esc(value: string): string {
  const entities: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
  return value.replace(/[&<>"']/g, char => entities[char]);
}
function setError(message = '') {
  const el = document.querySelector<HTMLElement>('#error');
  if (el) { el.textContent = message; el.hidden = !message; }
}
function toast(message: string) {
  const el = document.querySelector<HTMLElement>('#toast');
  if (!el) return;
  el.textContent = message; el.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => { el.hidden = true; }, 1800);
}
function when(value: string) {
  const date = /^\d+$/.test(value) ? new Date(Number(value)) : new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  return new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(date);
}
function titleOf(note: NoteMeta) { return note.title || 'Untitled note'; }
function changed() { return !!active && editText !== active.content; }
function ensureCanLeave() {
  if (!changed()) return true;
  return window.confirm('You have unsaved changes. Leave this note? Your draft will be kept.');
}
function shell() {
  document.body.dataset.route = route;
  void setCompactWindow(route === 'quick').catch(error => setError(`Could not resize the window: ${errorMessage(error)}`));
  const settingsButton = route === 'archive'
    ? `<button class="button primary new-button" id="new-note">${icon('plus')}<span>New note</span></button><button class="icon-button" id="settings-button" aria-label="Settings" title="Settings">${icon('settings')}</button>`
    : '';
  app.innerHTML = `
    <div class="window-shell">
      <header class="topbar">
        <a class="brand" href="#archive" aria-label="Notes library">${icon('file')}<span>Quicknote</span></a>
        <div class="topbar-actions">
          ${settingsButton}
          <span class="status-dot ${settings.notes_dir ? 'ready' : ''}" title="${settings.notes_dir ? 'Folder selected' : 'No folder selected'}"></span>
        </div>
      </header>
      <main id="view"></main>
      <div class="error-banner" id="error" role="alert" hidden></div>
      <div class="toast" id="toast" role="status" hidden></div>
    </div>`;
  app.querySelector('.brand')?.addEventListener('click', e => { e.preventDefault(); go('archive'); });
  app.querySelector('#settings-button')?.addEventListener('click', () => go('settings'));
  app.querySelector('#new-note')?.addEventListener('click', openQuickView);
  renderRoute();
}
function renderRoute() {
  const view = document.querySelector<HTMLElement>('#view')!;
  if (!settings.notes_dir && route !== 'settings') { route = 'settings'; }
  if (route === 'settings') return renderSettings(view);
  if (route === 'quick') return renderQuick(view);
  if (route === 'recent') return renderRecent(view);
  return renderArchive(view);
}
async function go(next: Route) {
  if (route === 'archive' && active && !ensureCanLeave()) return;
  if (route === 'archive' && active && !(await persistActiveDraft())) return;
  route = next; active = null; externalConflict = false; setError();
  shell();
}
async function refreshSettings() {
  settings = await api.getSettings();
}

function renderSettings(view: HTMLElement) {
  view.innerHTML = `
    <section class="settings-page">
      <button class="back-link" id="settings-back">${icon('left')} <span>Back to notes</span></button>
      <div class="page-heading"><h1>Settings</h1><p>Choose your appearance, notes folder, and shortcuts.</p></div>
      <section class="setting-card appearance-card">
        <div class="setting-copy"><span class="setting-icon">${icon('appearance')}</span><div><h2>Appearance</h2><p>System follows your device’s light or dark appearance.</p></div></div>
        <div class="theme-choice"><label for="theme-select">Theme</label><select id="theme-select" aria-describedby="theme-hint">${themes.map(theme => `<option value="${theme.id}" ${currentTheme() === theme.id ? 'selected' : ''}>${theme.label}</option>`).join('')}</select></div>
        <div class="theme-preview" aria-hidden="true"><span class="theme-swatch surface"></span><span class="theme-swatch sidebar"></span><span class="theme-swatch accent"></span><span class="theme-swatch text"></span><span>Aa</span></div>
        <p class="hint" id="theme-hint">Applied immediately. Saved on this device.</p>
      </section>
      <section class="setting-card">
        <div class="setting-copy"><span class="setting-icon">${icon('folder')}</span><div><h2>Notes folder</h2><p>Each note is stored as a separate Markdown file.</p></div></div>
        <div class="folder-choice"><div class="folder-path ${settings.notes_dir ? '' : 'muted'}" title="${esc(settings.notes_dir || '')}">${esc(settings.notes_dir || 'No folder selected')}</div><button class="button secondary" id="choose-folder">${settings.notes_dir ? 'Change folder' : 'Choose folder'}</button></div>
      </section>
      <section class="setting-card shortcuts-card">
        <div class="setting-copy"><span class="setting-icon">${icon('keyboard')}</span><div><h2>Keyboard shortcuts</h2><p>Set two shortcuts independently. Conflicts cannot be saved.</p></div></div>
        <label class="shortcut-row"><span><strong>New note</strong><small>Open an empty note</small></span><input id="shortcut-new" class="shortcut-input" aria-label="New note shortcut" placeholder="No shortcut set" value="${esc(settings.shortcut_new_note || '')}" readonly></label>
        <label class="shortcut-row"><span><strong>All notes</strong><small>Show every note, most recently edited first</small></span><input id="shortcut-recent" class="shortcut-input" aria-label="All notes shortcut" placeholder="No shortcut set" value="${esc(settings.shortcut_recent_notes || '')}" readonly></label>
        <div class="shortcut-actions"><span class="hint">Select a field and press a key combination.</span><button class="button primary" id="save-shortcuts">Save shortcuts</button></div>
        ${shortcutStatus.available ? '' : `<p class="shortcut-status">${esc(shortcutStatus.message || 'Global shortcuts are unavailable in this desktop environment. Open notes from the system tray.')}</p>`}
      </section>
      <p class="privacy-note">Your notes stay in the folder you choose.</p>
    </section>`;
  view.querySelector('#settings-back')?.addEventListener('click', () => go('archive'));
  view.querySelector('#choose-folder')?.addEventListener('click', chooseFolder);
  view.querySelector<HTMLSelectElement>('#theme-select')?.addEventListener('change', event => {
    const select = event.currentTarget as HTMLSelectElement;
    try { chooseTheme(select.value); setError(); }
    catch (error) { select.value = currentTheme(); setError(`Could not save theme: ${errorMessage(error)}`); }
  });
  bindShortcutInput(view.querySelector<HTMLInputElement>('#shortcut-new')!);
  bindShortcutInput(view.querySelector<HTMLInputElement>('#shortcut-recent')!);
  view.querySelector('#save-shortcuts')?.addEventListener('click', saveShortcuts);
}
function bindShortcutInput(input: HTMLInputElement) {
  input.addEventListener('keydown', event => {
    event.preventDefault();
    if (event.key === 'Escape' || event.key === 'Backspace' || event.key === 'Delete') { input.value = ''; return; }
    if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) return;
    const parts: string[] = [];
    if (event.ctrlKey || event.metaKey) parts.push('Ctrl');
    if (event.altKey) parts.push('Alt');
    if (event.shiftKey) parts.push('Shift');
    const key = event.key === ' ' ? 'Space' : event.key.length === 1 ? event.key.toUpperCase() : event.key;
    if (!parts.length && key.length === 1) { toast('Add Ctrl, Alt, or Shift to create a shortcut.'); return; }
    parts.push(key); input.value = parts.join('+');
  });
}
async function saveShortcuts() {
  const fresh = document.querySelector<HTMLInputElement>('#shortcut-new')!.value.trim() || null;
  const recent = document.querySelector<HTMLInputElement>('#shortcut-recent')!.value.trim() || null;
  if (fresh && recent && fresh.toLowerCase() === recent.toLowerCase()) { setError('These shortcuts cannot be the same.'); return; }
  try {
    await api.setShortcuts(fresh, recent);
    settings.shortcut_new_note = fresh; settings.shortcut_recent_notes = recent;
    toast('Shortcuts saved.'); setError();
  } catch (error) { setError(`Could not save shortcuts: ${errorMessage(error)}`); }
}
async function chooseFolder() {
  try {
    const selected = await open({ directory: true, multiple: false, title: 'Choose notes folder' });
    if (typeof selected !== 'string') return;
    if (!(await persistActiveDraft())) return;
    const drafts = await api.listNoteDrafts();
    if (drafts.length && !window.confirm(`You have ${drafts.length} note drafts. Delete them before switching folders?`)) return;
    for (const draft of drafts) await api.discardNoteDraft(draft.id);
    await api.setNotesDir(selected);
    settings.notes_dir = selected; active = null; notes = []; draftIds.clear();
    shell(); toast('Notes folder updated.');
  } catch (error) { setError(`Could not choose folder: ${errorMessage(error)}`); }
}

function renderArchive(view: HTMLElement) {
  view.innerHTML = `<section class="archive-page">
    <div class="archive-layout">
      <aside class="note-sidebar"><label class="search-box">${icon('search')}<input id="search" type="search" placeholder="Search notes" value="${esc(searchText)}" autocomplete="off"><kbd>Ctrl+K</kbd></label><div class="list-caption"><span>ALL NOTES <small class="list-key-hint">↑/↓ · J/K</small></span><span>${notes.length}</span></div><div class="note-list" id="note-list">${renderNoteRows(notes)}</div></aside>
      <section class="note-workspace" id="workspace">${active ? renderNoteEditor(active) : renderEmptyWorkspace()}</section>
    </div></section>`;
  view.querySelector('#new-note')?.addEventListener('click', openQuickView);
  const search = view.querySelector<HTMLInputElement>('#search')!;
  search.addEventListener('input', () => { searchText = search.value; debounceSearch(); });
  view.querySelector('#note-list')?.addEventListener('click', e => {
    const row = (e.target as HTMLElement).closest<HTMLElement>('[data-note-id]');
    if (row) openArchiveNote(row.dataset.noteId!);
  });
  bindEditor(view);
  search.onkeydown = event => { if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') event.preventDefault(); };
}
let searchTimer: number | undefined;
function debounceSearch() {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(async () => { try { notes = await api.listNotes(searchText); const list = document.querySelector('#note-list'); if (list) list.innerHTML = renderNoteRows(notes); } catch (e) { setError(errorMessage(e)); } }, 180);
}
function renderNoteRows(rows: NoteMeta[]) {
  if (!rows.length) return `<div class="list-empty">${searchText ? 'No matching notes found.' : 'No notes yet.'}</div>`;
  return rows.map(n => `<button class="note-row ${active?.meta.id === n.id ? 'selected' : ''}" data-note-id="${esc(n.id)}"><span class="row-title">${esc(titleOf(n))}${draftIds.has(n.id) ? '<small class="draft-badge">draft</small>' : ''}</span><span class="row-date">${esc(when(n.updated_at))}</span><span class="row-preview">${esc(n.preview || n.path.split('/').pop() || 'Markdown note')}</span></button>`).join('');
}
function renderEmptyWorkspace() {
  return `<div class="empty-workspace"><div class="empty-illustration"><span class="paper-line"></span><span class="paper-line short"></span><span class="paper-line"></span><span class="paper-fold"></span></div><h2>Select a note</h2><p>Open a note from the list, or create a new one.</p><button class="button secondary" id="empty-new">${icon('plus')} New note</button></div>`;
}
async function openArchiveNote(id: string) {
  if (active?.meta.id === id) return;
  if (!ensureCanLeave()) return;
  if (!(await persistActiveDraft())) return;
  active = null;
  try {
    active = await api.readNote(id);
    const draft = await api.getNoteDraft(id);
    editText = draft?.content ?? active.content;
    const currentRevision = active.revision;
    if (draft?.base_revision) active.revision = draft.base_revision;
    externalConflict = !!draft?.base_revision && draft.base_revision !== currentRevision;
    const workspace = document.querySelector<HTMLElement>('#workspace');
    if (workspace) workspace.innerHTML = renderNoteEditor(active);
    const list = document.querySelector('#note-list'); if (list) list.innerHTML = renderNoteRows(notes);
    bindEditor(document.querySelector('#view')!);
  } catch (error) { setError(`Could not open note: ${errorMessage(error)}`); }
}
function renderNoteEditor(note: NoteRecord) {
  return `<div class="editor-pane"><div class="editor-toolbar"><div class="editor-file">${icon('file')}<span>${esc(titleOf(note.meta))}</span></div><div class="editor-actions"><button class="icon-button delete-button" id="delete-note" title="Delete note" aria-label="Delete note">${icon('trash')}</button><button class="button primary save-button" id="save-note">Save</button></div></div><div class="editor-meta"><span>${esc(note.meta.path)}</span><span id="save-state">${changed() ? 'Saving draft' : 'Last edited ' + esc(when(note.meta.updated_at))}</span></div>${externalConflict ? '<div class="conflict-banner">This note changed elsewhere. Your draft is safe; review the content before saving.</div>' : ''}<textarea class="note-editor" id="note-editor" spellcheck="true" placeholder="Write your note…">${esc(editText)}</textarea><footer class="editor-footer"><span>Markdown supported</span><span><kbd>Ctrl</kbd> + <kbd>Enter</kbd> to save</span></footer></div>`;
}
function bindEditor(root: ParentNode) {
  const emptyNew = root.querySelector('#empty-new');
  emptyNew?.addEventListener('click', openQuickView);
  const editor = root.querySelector<HTMLTextAreaElement>('#note-editor');
  editor?.addEventListener('input', () => {
    editText = editor.value;
    const state = document.querySelector('#save-state'); if (state) state.textContent = 'Saving draft';
    window.clearTimeout(noteDraftTimer);
    if (active) {
      const id = active.meta.id;
      const content = editText;
      const baseRevision = active.revision;
      noteDraftTimer = window.setTimeout(async () => {
      try { await api.saveNoteDraft(id, content, baseRevision); const el = document.querySelector('#save-state'); if (el && active?.meta.id === id) el.textContent = 'Draft saved'; }
      catch (e) { setError(`Could not save draft: ${errorMessage(e)}`); }
      }, 250);
    }
  });
  editor?.addEventListener('keydown', event => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); saveActiveNote(); }
  });
  root.querySelector('#save-note')?.addEventListener('click', saveActiveNote);
  root.querySelector('#delete-note')?.addEventListener('click', deleteActiveNote);
}
async function persistActiveDraft(): Promise<boolean> {
  window.clearTimeout(noteDraftTimer);
  if (!active) return true;
  try {
    if (changed()) await api.saveNoteDraft(active.meta.id, editText, active.revision);
    else await api.discardNoteDraft(active.meta.id);
    return true;
  }
  catch (error) { setError(`Could not save draft: ${errorMessage(error)}`); return false; }
}
async function saveActiveNote() {
  if (!active || busy || !changed()) { if (!changed()) toast('No changes to save.'); return; }
  busy = true;
  try {
    window.clearTimeout(noteDraftTimer);
    const updated = await api.updateNote(active.meta.id, editText, active.revision);
    active = { meta: updated, content: editText, revision: updated.revision || active.revision };
    await api.discardNoteDraft(active.meta.id);
    draftIds.delete(active.meta.id);
    notes = await api.listNotes(searchText);
    externalConflict = false;
    const workspace = document.querySelector<HTMLElement>('#workspace'); if (workspace) workspace.innerHTML = renderNoteEditor(active);
    const list = document.querySelector('#note-list'); if (list) list.innerHTML = renderNoteRows(notes);
    bindEditor(document.querySelector('#view')!); toast('Note saved.'); setError();
  } catch (error) {
    externalConflict = true;
    if (active) {
      try { await api.saveNoteDraft(active.meta.id, editText, active.revision); draftIds.add(active.meta.id); }
      catch { /* keep the current edit visible if persistence also fails */ }
    }
    setError(`Could not save note: ${errorMessage(error)}. Your draft was kept.`);
    const workspace = document.querySelector<HTMLElement>('#workspace'); if (workspace && active) workspace.innerHTML = renderNoteEditor(active);
  }
  finally { busy = false; }
}
async function deleteActiveNote() {
  if (!active || !window.confirm(`Permanently delete “${titleOf(active.meta)}”?`)) return;
  try {
    await api.deleteNote(active.meta.id, active.revision);
    await api.discardNoteDraft(active.meta.id); draftIds.delete(active.meta.id);
    active = null; editText = ''; notes = await api.listNotes(searchText); renderRoute(); toast('Note deleted.');
  } catch (error) { externalConflict = true; setError(`Could not delete note: ${errorMessage(error)}`); }
}

function renderQuick(view: HTMLElement) {
  view.innerHTML = `<section class="compact-page quick-page"><div class="compact-heading"><h1>New note</h1><button class="icon-button" id="compact-close" aria-label="Close">${icon('close')}</button></div><textarea id="quick-editor" class="quick-editor" placeholder="Write an idea…" autofocus spellcheck="true">${esc(quickText)}</textarea><div class="compact-footer"><span class="save-indicator" id="quick-state">${quickText ? 'Draft saved' : 'Ready when you are'}</span><span><kbd>Enter</kbd> to save <i>·</i> <kbd>Shift</kbd> <kbd>Enter</kbd> for a new line</span></div></section>`;
  const editor = view.querySelector<HTMLTextAreaElement>('#quick-editor')!;
  editor.focus(); editor.setSelectionRange(editor.value.length, editor.value.length);
  editor.addEventListener('input', () => {
    quickText = editor.value;
    document.querySelector('#quick-state')!.textContent = 'Saving draft';
    window.clearTimeout(quickTimer);
    quickTimer = window.setTimeout(async () => { try { await api.saveDraft(quickText); const state = document.querySelector('#quick-state'); if (state) state.textContent = 'Draft saved'; } catch (e) { setError(`Could not save draft: ${errorMessage(e)}`); } }, 150);
  });
  editor.addEventListener('keydown', async event => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Escape') { event.preventDefault(); await persistQuick(); await api.hideWindow(); return; }
    if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); if (editor.value.trim()) await saveQuick(); }
  });
  view.querySelector('#compact-close')?.addEventListener('click', async () => { await persistQuick(); await api.hideWindow(); });
  editor.addEventListener('blur', () => { window.setTimeout(async () => { if (route === 'quick') { await persistQuick(); await api.hideWindow(); } }, 100); });
}
async function openQuickView() {
  if (route === 'archive' && active && !ensureCanLeave()) return;
  if (route === 'archive' && active && !(await persistActiveDraft())) return;
  try { quickText = await api.loadDraft(); }
  catch (error) { setError(`Could not open draft: ${errorMessage(error)}`); quickText = ''; }
  active = null; route = 'quick'; shell();
}
async function persistQuick() { window.clearTimeout(quickTimer); try { await api.saveDraft(quickText); } catch (e) { setError(`Could not save draft: ${errorMessage(e)}`); } }
async function saveQuick() {
  if (busy || !quickText.trim()) return;
  window.clearTimeout(quickTimer);
  busy = true;
  try { await api.saveNote(quickText); quickText = ''; await api.hideWindow(); setError(); }
  catch (error) {
    try { await api.saveDraft(quickText); } catch { /* keep the visible draft for another attempt */ }
    setError(`Could not save note: ${errorMessage(error)}`);
  }
  finally { busy = false; }
}

function renderRecent(view: HTMLElement) {
  if (recentEditing) return renderRecentEditor(view);
  view.innerHTML = `<section class="compact-page recent-page" tabindex="-1"><div class="compact-heading"><h1>All notes</h1><button class="icon-button" id="compact-close" aria-label="Close">${icon('close')}</button></div><div class="recent-list" id="recent-list">${renderRecentRows()}</div><div class="compact-footer"><span>Select a note to edit</span><span><kbd>↑</kbd>/<kbd>↓</kbd> <kbd>J</kbd>/<kbd>K</kbd> navigate <i>·</i> <kbd>Enter</kbd> open <i>·</i> <kbd>X</kbd> delete</span></div></section>`;
  view.querySelector<HTMLElement>('.recent-page')?.focus();
  view.querySelector('#compact-close')?.addEventListener('click', () => api.hideWindow());
  view.querySelector('.recent-page')?.addEventListener('focusout', () => { window.setTimeout(() => {
    if (route === 'recent' && !recentEditing && !recentDeletePending && !document.querySelector('.recent-page')?.contains(document.activeElement)) api.hideWindow();
  }, 100); });
  view.querySelector('#recent-list')?.addEventListener('click', e => { const row = (e.target as HTMLElement).closest<HTMLElement>('[data-recent-id]'); if (row) selectRecent(Number(row.dataset.index)); });
  view.addEventListener('keydown', recentKeydown);
}
function renderRecentEditor(view: HTMLElement) {
  view.innerHTML = `<section class="compact-page recent-edit-page"><div class="compact-heading"><button class="back-link" id="recent-back">${icon('left')} <span>All notes</span></button><button class="icon-button" id="compact-close" aria-label="Close">${icon('close')}</button></div><div class="recent-edit-title">${esc(titleOf(recentEditing!.meta))}</div>${externalConflict ? '<div class="conflict-banner">This note changed elsewhere. Your draft is safe; saving may conflict.</div>' : ''}<textarea id="recent-editor" class="recent-editor" spellcheck="true">${esc(recentEditText)}</textarea><div class="compact-footer"><span id="recent-save-state">Your edits are saved as a draft</span><span><kbd>Enter</kbd> to update <i>·</i> <kbd>Shift</kbd> <kbd>Enter</kbd> for a new line</span></div></section>`;
  const editor = view.querySelector<HTMLTextAreaElement>('#recent-editor')!;
  editor.focus(); editor.setSelectionRange(editor.value.length, editor.value.length);
  editor.addEventListener('input', () => {
    recentEditText = editor.value;
    const state = document.querySelector('#recent-save-state'); if (state) state.textContent = 'Saving draft';
    window.clearTimeout(noteDraftTimer);
    const id = recentEditing!.meta.id;
    const revision = recentEditing!.revision;
    const content = recentEditText;
    noteDraftTimer = window.setTimeout(async () => {
      try { await api.saveNoteDraft(id, content, revision); const el = document.querySelector('#recent-save-state'); if (el) el.textContent = 'Draft saved'; }
      catch (error) { setError(`Could not save draft: ${errorMessage(error)}`); }
    }, 250);
  });
  editor.addEventListener('keydown', event => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Escape') { event.preventDefault(); persistRecentDraft().then(async saved => { if (saved) await api.hideWindow(); }); }
    if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); saveRecentEdit(); }
  });
  view.querySelector('#recent-back')?.addEventListener('click', async () => { if (await persistRecentDraft()) { recentEditing = null; renderRoute(); } });
  view.querySelector('#compact-close')?.addEventListener('click', async () => { if (await persistRecentDraft()) await api.hideWindow(); });
  editor.addEventListener('blur', () => { window.setTimeout(async () => {
    if (route === 'recent' && !document.querySelector('.recent-edit-page')?.contains(document.activeElement)) {
      if (await persistRecentDraft()) await api.hideWindow();
    }
  }, 100); });
}
async function persistRecentDraft(): Promise<boolean> {
  window.clearTimeout(noteDraftTimer);
  if (!recentEditing) return true;
  try {
    if (recentEditText !== recentEditing.content) { await api.saveNoteDraft(recentEditing.meta.id, recentEditText, recentEditing.revision); draftIds.add(recentEditing.meta.id); }
    else { await api.discardNoteDraft(recentEditing.meta.id); draftIds.delete(recentEditing.meta.id); }
    return true;
  } catch (error) { setError(`Could not save draft: ${errorMessage(error)}`); return false; }
}
async function saveRecentEdit() {
  if (!recentEditing || busy) return;
  busy = true; window.clearTimeout(noteDraftTimer);
  try {
    await api.updateNote(recentEditing.meta.id, recentEditText, recentEditing.revision);
    await api.discardNoteDraft(recentEditing.meta.id);
    draftIds.delete(recentEditing.meta.id);
    notes = await api.getRecentNotes();
    recentEditing = null; recentEditText = ''; externalConflict = false;
    await api.hideWindow();
  } catch (error) {
    if (recentEditing) await api.saveNoteDraft(recentEditing.meta.id, recentEditText, recentEditing.revision).then(() => draftIds.add(recentEditing!.meta.id)).catch(() => undefined);
    setError(`Could not update note: ${errorMessage(error)}. Your draft was kept.`);
    const view = document.querySelector<HTMLElement>('#view'); if (view) renderRecentEditor(view);
  } finally { busy = false; }
}
function renderRecentRows() {
  if (!notes.length) return '<div class="recent-empty">No notes yet.</div>';
  return notes.map((n, i) => `<button class="recent-row ${i === selectedRecent ? 'active' : ''}" data-recent-id="${esc(n.id)}" data-index="${i}"><span class="recent-marker">${i === selectedRecent ? '↵' : ''}</span><span class="recent-copy"><strong>${esc(titleOf(n))}${draftIds.has(n.id) ? '<small class="draft-badge">draft</small>' : ''}</strong><small>${esc(n.path.split('/').pop() || '')}</small></span><time>${esc(when(n.updated_at))}</time><span class="row-arrow">${icon('right')}</span></button>`).join('');
}
function recentKeydown(event: KeyboardEvent) {
  if (recentEditing || recentDeletePending) return;
  if (event.isComposing || event.keyCode === 229 || event.ctrlKey || event.metaKey || event.altKey) return;
  const target = event.target;
  if (target instanceof HTMLElement && (target.isContentEditable || target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])'))) return;
  if (event.key === 'Escape') { event.preventDefault(); api.hideWindow(); }
  if (event.key === 'ArrowDown' || event.key.toLowerCase() === 'j') { event.preventDefault(); selectedRecent = notes.length ? Math.min(notes.length - 1, selectedRecent + 1) : 0; updateRecentList(); }
  if (event.key === 'ArrowUp' || event.key.toLowerCase() === 'k') { event.preventDefault(); selectedRecent = Math.max(0, selectedRecent - 1); updateRecentList(); }
  if (event.key === 'Enter' && notes.length) { event.preventDefault(); selectRecent(selectedRecent); }
  if (event.key.toLowerCase() === 'x' && !event.repeat && notes.length) { event.preventDefault(); void deleteSelectedRecent(); }
}
async function deleteSelectedRecent() {
  if (recentDeletePending || recentEditing || notes.length === 0) return;
  const note = notes[selectedRecent];
  if (!note) return;
  const noteId = note.id;
  const revision = note.revision;
  if (!revision) { setError('Could not delete note: its revision is unavailable.'); return; }
  const hadDraft = draftIds.has(noteId);
  recentDeletePending = true;
  try {
    const confirmed = await confirm(
      hadDraft
        ? `Permanently delete “${titleOf(note)}” and its saved draft? This action cannot be undone.`
        : `Permanently delete “${titleOf(note)}”? This action cannot be undone.`,
      { title: 'Delete note', kind: 'warning', okLabel: 'Delete', cancelLabel: 'Cancel' },
    );
    if (!confirmed) return;
    try {
      await api.deleteNote(noteId, revision);
    } catch (error) {
      setError(`Could not delete note: ${errorMessage(error)}`);
      return;
    }

    notes = notes.filter(item => item.id !== noteId);
    selectedRecent = Math.min(selectedRecent, Math.max(0, notes.length - 1));
    draftIds.delete(noteId);
    updateRecentList();
    setError();
    if (hadDraft) {
      try { await api.discardNoteDraft(noteId); }
      catch (error) { setError(`Note deleted, but its saved draft could not be removed: ${errorMessage(error)}`); }
    }
    try {
      notes = await api.getRecentNotes();
      selectedRecent = Math.min(selectedRecent, Math.max(0, notes.length - 1));
      updateRecentList();
    } catch (error) { setError(`Note was deleted, but the list could not refresh: ${errorMessage(error)}`); }
  } catch (error) {
    setError(`Could not open the delete confirmation: ${errorMessage(error)}`);
  } finally {
    recentDeletePending = false;
    document.querySelector<HTMLElement>('.recent-page')?.focus();
  }
}
function updateRecentList() {
  const list = document.querySelector('#recent-list'); if (list) list.innerHTML = renderRecentRows();
  document.querySelector('.recent-row.active')?.scrollIntoView({ block: 'nearest' });
}
async function selectRecent(index: number) {
  const note = notes[index]; if (!note) return;
  try {
    const record = await api.readNote(note.id);
    const draft = await api.getNoteDraft(note.id);
    recentEditing = record; recentEditText = draft?.content ?? record.content;
    if (draft) draftIds.add(note.id);
    externalConflict = !!draft?.base_revision && draft.base_revision !== record.revision;
    if (draft?.base_revision) recentEditing.revision = draft.base_revision;
    renderRoute();
  } catch (error) { setError(`Could not open note: ${errorMessage(error)}`); }
}

function icon(name: string) {
  const shapes: Record<string, string> = {
    appearance: '<circle cx="12" cy="12" r="9"/><path d="M12 3v18"/><path d="M12 3a9 9 0 0 1 0 18Z" fill="currentColor" stroke="none"/>',
    settings: '<circle cx="12" cy="12" r="3"/><path d="m19.4 15 .1.1 1.4 1.1-1.4 2.4-1.7-.7a7.6 7.6 0 0 1-1.6.9l-.3 1.8h-2.8l-.3-1.8a7.6 7.6 0 0 1-1.6-.9l-1.7.7-1.4-2.4L7.5 15a7.7 7.7 0 0 1 0-1.9L6.1 12l1.4-2.4 1.7.7a7.6 7.6 0 0 1 1.6-.9l.3-1.8h2.8l.3 1.8a7.6 7.6 0 0 1 1.6.9l1.7-.7 1.4 2.4-1.4 1.1a7.7 7.7 0 0 1-.1 1.9Z"/>',
    folder: '<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H10l2 2h7.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z"/>',
    keyboard: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 9h.01M10 9h.01M13 9h.01M16 9h.01M7 12h.01M10 12h.01M13 12h.01M16 12h.01M8 15h8"/>',
    left: '<path d="m15 18-6-6 6-6"/>', plus: '<path d="M12 5v14M5 12h14"/>', search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/>',
    file: '<path d="M13 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V10z"/><path d="M13 3v7h7M8 14h8M8 17h6"/>',
    trash: '<path d="M3 6h18M8 6V4h8v2m3 0-1 14H6L5 6m4 4v6m6-6v6"/>', close: '<path d="m18 6-12 12M6 6l12 12"/>', right: '<path d="m9 18 6-6-6-6"/>'
  };
  return `<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${shapes[name] || ''}</svg>`;
}

async function start() {
  try {
    window.addEventListener('keydown', event => {
      if (route !== 'archive') return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault(); document.querySelector<HTMLInputElement>('#search')?.focus();
        return;
      }
      if (event.isComposing || event.keyCode === 229 || event.ctrlKey || event.metaKey || event.altKey) return;
      const target = event.target;
      if (target instanceof HTMLElement && (target.isContentEditable || target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])'))) return;
      const key = event.key.toLowerCase();
      const down = key === 'j' || event.key === 'ArrowDown';
      const up = key === 'k' || event.key === 'ArrowUp';
      if ((!down && !up) || notes.length === 0) return;
      event.preventDefault();
      const currentIndex = active ? notes.findIndex(note => note.id === active?.meta.id) : -1;
      const nextIndex = currentIndex < 0
        ? (down ? 0 : notes.length - 1)
        : Math.max(0, Math.min(notes.length - 1, currentIndex + (down ? 1 : -1)));
      void openArchiveNote(notes[nextIndex].id);
    });
    await refreshSettings();
    shortcutStatus = await api.getShortcutStatus();
    draftIds = new Set((await api.listNoteDrafts()).map(draft => draft.id));
    if (route === 'quick') quickText = await api.loadDraft();
    if (route === 'recent') { notes = await api.getRecentNotes(); selectedRecent = 0; }
    if (route === 'archive' && settings.notes_dir) notes = await api.listNotes();
    shell();
    await api.listen('open-quick-note', async () => {
      if (route === 'archive' && active && !(await persistActiveDraft())) return;
    if (route === 'recent' && recentEditing && !(await persistRecentDraft())) return;
      if (!settings.notes_dir) { route = 'settings'; shell(); return; }
      route = 'quick'; active = null; quickText = await api.loadDraft(); shell();
    });
    await api.listen('open-recent-notes', async () => {
      if (route === 'archive' && active && !(await persistActiveDraft())) return;
      if (route === 'recent' && recentEditing && !(await persistRecentDraft())) return;
      if (!settings.notes_dir) { route = 'settings'; shell(); return; }
      route = 'recent'; active = null; notes = await api.getRecentNotes(); draftIds = new Set((await api.listNoteDrafts()).map(draft => draft.id)); selectedRecent = 0; recentEditing = null; shell();
    });
    await api.listen('open-archive', async () => {
      if (route === 'archive' && active && !(await persistActiveDraft())) return;
      if (route === 'recent' && recentEditing && !(await persistRecentDraft())) return;
      if (!settings.notes_dir) { route = 'settings'; shell(); return; }
      route = 'archive'; active = null; notes = await api.listNotes(searchText); draftIds = new Set((await api.listNoteDrafts()).map(draft => draft.id)); shell();
    });
    await api.listen('open-settings', async () => {
      if (route === 'archive' && active && !(await persistActiveDraft())) return;
      if (route === 'recent' && recentEditing && !(await persistRecentDraft())) return;
      route = 'settings'; active = null; shell();
    });
    await api.listen('notes-changed', async () => {
      try {
        if (route === 'recent') {
          notes = await api.getRecentNotes(); draftIds = new Set((await api.listNoteDrafts()).map(draft => draft.id)); selectedRecent = Math.min(selectedRecent, Math.max(0, notes.length - 1));
          if (recentEditing) {
            const fresh = await api.readNote(recentEditing.meta.id);
            if (fresh.revision !== recentEditing.revision) externalConflict = true;
          }
          renderRoute();
        }
        else if (route === 'archive') {
          notes = await api.listNotes(searchText); draftIds = new Set((await api.listNoteDrafts()).map(draft => draft.id));
          if (active) {
            const fresh = await api.readNote(active.meta.id);
            if (fresh.revision !== active.revision) { externalConflict = true; }
          }
          renderRoute();
        }
      } catch (error) { setError(`Notlar yenilenemedi: ${errorMessage(error)}`); }
    });
    await api.listen('shortcut-status', async () => {
      try { shortcutStatus = await api.getShortcutStatus(); if (route === 'settings') renderRoute(); }
      catch (error) { setError(`Could not read shortcut status: ${errorMessage(error)}`); }
    });
    await api.reportFrontendReady();
  } catch (error) { shell(); setError(`Could not start the app: ${errorMessage(error)}`); }
}

start();
