import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface Settings {
  config_path: string;
  use_yesterday_if_today_missing: boolean;
}

export interface SavedEntry {
  note_path: string;
}

export interface JournalApi {
  loadSettings(): Promise<Settings>;
  setFallback(enabled: boolean): Promise<Settings>;
  submit(text: string): Promise<SavedEntry>;
  exit(): Promise<void>;
  startDragging(): Promise<void>;
}

export const api: JournalApi = {
  loadSettings: () => invoke<Settings>("load_settings"),
  setFallback: (enabled) => invoke<Settings>("set_fallback", { enabled }),
  submit: (text) => invoke<SavedEntry>("submit_entry", { text }),
  exit: () => invoke<void>("request_exit"),
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
