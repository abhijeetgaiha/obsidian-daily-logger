import { invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** The window size set in tauri.conf.json. */
export const WINDOW_WIDTH = 520;
export const WINDOW_HEIGHT = 300;

// Codes for problems fixed in the Settings dialog; the main window shows only "Config error!".
const CONFIG_ERRORS = new Set([
  "configuration", "note_source_unset", "not_a_vault", "no_daily_notes_plugin",
  "plugin_disabled", "plugin_settings", "date_format", "note_path",
]);

export function isConfigError(error: unknown): boolean {
  return typeof error === "object" && error !== null && "code" in error &&
    typeof error.code === "string" && CONFIG_ERRORS.has(error.code);
}

export interface NoteLabel {
  name: string;
  is_yesterday: boolean;
}

export interface Settings {
  config_path: string;
  use_yesterday_if_today_missing: boolean;
  entry_format: EntryFormat;
  note: NoteLabel | null;
}

export type DuplicateHeading = "error" | "first" | "last";

export type EntryFormat = "inline" | "block";

export type NoteSource = "periodic" | "daily";

export interface SettingsForm {
  vault_root: string | null;
  note_source: NoteSource | null;
  heading: string;
  duplicate_heading: DuplicateHeading;
  entry_format: EntryFormat;
  use_yesterday_if_today_missing: boolean;
}

export interface FormResult {
  config_path: string;
  exists: boolean;
  form: SettingsForm;
  issue: string | null;
}

export interface HeadingList {
  note: string | null;
  headings: string[];
  problem: string | null;
  detected: NoteSource | null;
  layout: string | null;
}

export interface SavedEntry {
  note_path: string;
}

/** A vault note offered after `[[`. */
export interface NoteLink {
  name: string;
  /** Vault-relative folder, empty for the vault root. */
  folder: string;
  /** The text inserted between `[[` and `]]`, in the vault's link format. */
  link: string;
}

export interface NoteIndex {
  notes: NoteLink[];
  problem: string | null;
}

export interface JournalApi {
  loadDraft(): Promise<string>;
  saveDraft(text: string): Promise<void>;
  loadSettings(): Promise<Settings>;
  setFallback(enabled: boolean): Promise<Settings>;
  setEntryFormat(format: EntryFormat): Promise<Settings>;
  readSettingsForm(): Promise<FormResult>;
  listHeadings(vaultRoot: string | null, noteSource: NoteSource | null): Promise<HeadingList>;
  /** The vault's notes for `[[` completion; configuration problems give an empty list. */
  listNotes(): Promise<NoteIndex>;
  pickVaultFolder(current: string | null): Promise<string | null>;
  saveSettings(form: SettingsForm): Promise<Settings>;
  submit(text: string): Promise<SavedEntry>;
  exit(text?: string): Promise<void>;
  startDragging(): Promise<void>;
  /** Sets the window height in logical pixels; the width stays fixed. */
  setWindowHeight(height: number): Promise<void>;
  /** Called when the OS asks to close or quit; resolves to an unsubscribe function. */
  onCloseRequested(handler: () => void): Promise<() => void>;
}

export const api: JournalApi = {
  loadDraft: () => invoke<string>("load_draft"),
  saveDraft: (text) => invoke<void>("save_draft", { text }),
  loadSettings: () => invoke<Settings>("load_settings"),
  setFallback: (enabled) => invoke<Settings>("set_fallback", { enabled }),
  setEntryFormat: (format) => invoke<Settings>("set_entry_format", { format }),
  readSettingsForm: () => invoke<FormResult>("read_settings_form"),
  listHeadings: (vaultRoot, noteSource) =>
    invoke<HeadingList>("list_headings", { vaultRoot, noteSource }),
  listNotes: () => invoke<NoteIndex>("list_notes"),
  pickVaultFolder: (current) => invoke<string | null>("pick_vault_folder", { current }),
  saveSettings: (form) => invoke<Settings>("save_settings", { form }),
  submit: (text) => invoke<SavedEntry>("submit_entry", { text }),
  exit: (text) => invoke<void>("request_exit", { text: text ?? null }),
  startDragging: () => getCurrentWindow().startDragging(),
  setWindowHeight: (height) =>
    getCurrentWindow().setSize(new LogicalSize(WINDOW_WIDTH, height)),
  onCloseRequested: (handler) => listen("close-requested", handler),
};

export function errorMessage(error: unknown): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return error.message;
  }
  if (typeof error === "string") return error;
  return "Unexpected application error. Check the note before retrying.";
}
