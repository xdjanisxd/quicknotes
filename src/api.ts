import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { LogicalSize, type PhysicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';

export interface NoteMeta {
  id: string;
  title: string;
  path: string;
  updated_at: string;
  preview?: string;
  revision: string;
}

export interface NoteRecord {
  meta: NoteMeta;
  content: string;
  revision: string;
}
export interface NoteDraft {
  content: string;
  base_revision: string | null;
  updated_at: string;
}
export interface NoteDraftMeta {
  id: string;
  updated_at: string;
  base_revision: string | null;
}
export interface ShortcutStatus {
  available: boolean;
  message: string | null;
}

export interface AppSettings {
  notes_dir: string | null;
  shortcut_new_note: string | null;
  shortcut_recent_notes: string | null;
}

let savedWindowSize: PhysicalSize | null = null;
let compactWindow = false;
let windowResizeQueue: Promise<void> = Promise.resolve();

export function setCompactWindow(compact: boolean): Promise<void> {
  windowResizeQueue = windowResizeQueue.catch(() => undefined).then(async () => {
    if (compact === compactWindow) return;
    const window = getCurrentWindow();
    if (compact) {
      savedWindowSize = await window.outerSize();
      await window.setMinSize(new LogicalSize(400, 200));
      await window.setMinSize(new LogicalSize(440, 220));
      await window.setSize(new LogicalSize(500, 250));
      compactWindow = true;
      return;
    }
    if (savedWindowSize) {
      await window.setMinSize(new LogicalSize(400, 200));
      await window.setSize(savedWindowSize);
      savedWindowSize = null;
    }
    await window.setMinSize(new LogicalSize(420, 300));
    compactWindow = false;
  });
  return windowResizeQueue;
}

export const api = {
  getSettings: () => invoke<AppSettings>('get_settings'),
  setNotesDir: (path: string) => invoke<void>('set_notes_dir', { path }),
  setShortcuts: (newNote: string | null, recent: string | null) =>
    invoke<void>('set_shortcuts', { newNote, recent }),
  loadDraft: () => invoke<string>('load_draft'),
  saveDraft: (content: string) => invoke<void>('save_draft', { content }),
  saveNote: (content: string) => invoke<NoteMeta>('save_note', { content }),
  listNotes: (query?: string) => invoke<NoteMeta[]>('list_notes', { query: query || null }),
  getRecentNotes: () => invoke<NoteMeta[]>('get_recent_notes'),
  readNote: (id: string) => invoke<NoteRecord>('read_note', { id }),
  updateNote: (id: string, content: string, expectedRevision: string) =>
    invoke<NoteMeta>('update_note', { id, content, expectedRevision }),
  deleteNote: (id: string, expectedRevision: string) =>
    invoke<void>('delete_note', { id, expectedRevision }),
  getNoteDraft: (id: string) => invoke<NoteDraft | null>('get_note_draft', { id }),
  saveNoteDraft: (id: string, content: string, baseRevision: string | null) =>
    invoke<void>('save_note_draft', { id, content, baseRevision }),
  listNoteDrafts: () => invoke<NoteDraftMeta[]>('list_note_drafts'),
  discardNoteDraft: (id: string) => invoke<void>('discard_note_draft', { id }),
  getShortcutStatus: () => invoke<ShortcutStatus>('get_shortcut_status'),
  reportFrontendReady: () => invoke<void>('report_frontend_ready'),
  hideWindow: () => getCurrentWindow().hide(),
  listen: (event: string, handler: (payload?: unknown) => void): Promise<UnlistenFn> => listen(event, handler),
};

export function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return 'An unexpected error occurred.';
}
