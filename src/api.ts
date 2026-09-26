import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface NoteLabel {
  name: string;
  is_yesterday: boolean;
}

export interface Settings {
  config_path: string;
  use_yesterday_if_today_missing: boolean;
  note: NoteLabel | null;
}

export type DuplicateHeading = "error" | "first" | "last";

export interface SettingsForm {
  vault_root: string | null;
  heading: string;
  duplicate_heading: DuplicateHeading;
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
}

export interface SavedEntry {
  note_path: string;
}

export interface JournalApi {
  loadDraft(): Promise<string>;
  saveDraft(text: string): Promise<void>;
  loadSettings(): Promise<Settings>;
  setFallback(enabled: boolean): Promise<Settings>;
  readSettingsForm(): Promise<FormResult>;
  listHeadings(vaultRoot: string | null): Promise<HeadingList>;
  pickVaultFolder(current: string | null): Promise<string | null>;
  saveSettings(form: SettingsForm): Promise<Settings>;
  submit(text: string): Promise<SavedEntry>;
  exit(text?: string): Promise<void>;
  startDragging(): Promise<void>;
  /** Called when the OS asks to close or quit; resolves to an unsubscribe function. */
  onCloseRequested(handler: () => void): Promise<() => void>;
}

export const api: JournalApi = {
  loadDraft: () => invoke<string>("load_draft"),
  saveDraft: (text) => invoke<void>("save_draft", { text }),
  loadSettings: () => invoke<Settings>("load_settings"),
  setFallback: (enabled) => invoke<Settings>("set_fallback", { enabled }),
  readSettingsForm: () => invoke<FormResult>("read_settings_form"),
  listHeadings: (vaultRoot) => invoke<HeadingList>("list_headings", { vaultRoot }),
  pickVaultFolder: (current) => invoke<string | null>("pick_vault_folder", { current }),
  saveSettings: (form) => invoke<Settings>("save_settings", { form }),
  submit: (text) => invoke<SavedEntry>("submit_entry", { text }),
  exit: (text) => invoke<void>("request_exit", { text: text ?? null }),
  startDragging: () => getCurrentWindow().startDragging(),
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
