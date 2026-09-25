import { invoke } from "@tauri-apps/api/core";
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

export interface SavedEntry {
  note_path: string;
}

export interface JournalApi {
  loadDraft(): Promise<string>;
  loadSettings(): Promise<Settings>;
  setFallback(enabled: boolean): Promise<Settings>;
  submit(text: string): Promise<SavedEntry>;
  exit(text?: string): Promise<void>;
  startDragging(): Promise<void>;
}

export const api: JournalApi = {
  loadDraft: () => invoke<string>("load_draft"),
  loadSettings: () => invoke<Settings>("load_settings"),
  setFallback: (enabled) => invoke<Settings>("set_fallback", { enabled }),
  submit: (text) => invoke<SavedEntry>("submit_entry", { text }),
  exit: (text) => invoke<void>("request_exit", { text: text ?? null }),
  startDragging: () => getCurrentWindow().startDragging(),
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
