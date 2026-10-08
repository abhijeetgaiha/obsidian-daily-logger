import {
  errorMessage, isConfigError, WINDOW_HEIGHT, type JournalApi, type NoteLabel, type NoteLink,
  type Settings,
} from "./api";
import { createAutosave } from "./autosave";
import { continueList, createEditor, refreshLinkCompletion } from "./editor";
import { mountSettingsDialog } from "./settings";

const AUTOSAVE_NOTICE_MS = 1500;
/** A `[[` query reloads the note index in the background once it is older than this. */
export const NOTE_INDEX_MAX_AGE_MS = 30_000;

const gearIcon = `
  <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" focusable="false">
    <circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="2.6"
      stroke-dasharray="2.2 2.2" />
    <circle cx="8" cy="8" r="3.5" fill="none" stroke="currentColor" stroke-width="1.8" />
  </svg>
`;

export function mountJournal(root: HTMLElement, api: JournalApi): () => void {
  root.innerHTML = `
    <div id="journal">
      <div class="drag-area" aria-hidden="true"></div>
      <div id="entry"></div>
      <div class="status-row">
        <div class="toggles">
          <label class="fallback">
            <input id="fallback" type="checkbox" />
            Use yesterday if today is missing
          </label>
          <label class="toggle" title="Log as a bold time, the text, then a --- rule">
            <input id="block-format" type="checkbox" />
            Block format
          </label>
        </div>
        <div class="note">
          <span id="autosave" role="status" aria-live="polite"></span>
          <span id="note-name"></span>
          <button id="settings" type="button" aria-label="Settings" title="Settings"
            aria-haspopup="dialog">${gearIcon}</button>
        </div>
      </div>
      <p id="message" role="status" aria-live="polite"></p>
    </div>
  `;
  const journal = root.querySelector<HTMLDivElement>("#journal")!;
  const entry = root.querySelector<HTMLDivElement>("#entry")!;
  const checkbox = root.querySelector<HTMLInputElement>("#fallback")!;
  const blockFormat = root.querySelector<HTMLInputElement>("#block-format")!;
  const noteName = root.querySelector<HTMLSpanElement>("#note-name")!;
  const autosaveNotice = root.querySelector<HTMLSpanElement>("#autosave")!;
  const gear = root.querySelector<HTMLButtonElement>("#settings")!;
  const message = root.querySelector<HTMLParagraphElement>("#message")!;
  const dragArea = root.querySelector<HTMLDivElement>(".drag-area")!;
  let settings: Settings | undefined;
  let note: NoteLabel | undefined;
  let busy = true;
  let draftLoaded = false;
  let savedPath: string | undefined;
  let autosaveError: string | undefined;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  let notes: readonly NoteLink[] = [];
  let notesLoadedAt = -Infinity;
  let notesLoading = false;
  let notesStale = false;
  const editor = createEditor(entry, {
    placeholder: "Write a journal entry...",
    label: "Journal entry",
    onChange: () => autosave.schedule(),
    notes: () => notes,
    onLinkQuery() {
      if (!notesLoading && Date.now() - notesLoadedAt > NOTE_INDEX_MAX_AGE_MS) loadNotes();
    },
  });
  const autosave = createAutosave({
    read: () => editor.value(),
    state() {
      if (!draftLoaded || savedPath !== undefined) return "disabled";
      return busy || dialog.isOpen ? "blocked" : "ready";
    },
    save: (text) => api.saveDraft(text),
    onSaved() {
      if (autosaveError !== undefined && message.textContent === autosaveError) {
        message.textContent = "";
        message.className = "";
      }
      autosaveError = undefined;
      autosaveNotice.textContent = "Draft saved";
      clearTimeout(noticeTimer);
      noticeTimer = setTimeout(() => {
        autosaveNotice.textContent = "";
      }, AUTOSAVE_NOTICE_MS);
    },
    onError(error) {
      autosaveError = `Could not autosave the draft: ${errorMessage(error)}`;
      showError(autosaveError);
    },
  });
  let windowHeight = WINDOW_HEIGHT;
  function fitWindow(height: number) {
    if (height === windowHeight) return;
    windowHeight = height;
    // If resizing fails, the dialog still scrolls inside the window.
    api.setWindowHeight(height).catch((error) => console.warn("resize failed", error));
  }
  const dialog = mountSettingsDialog(root, api, {
    onSaved(saved) {
      settings = saved;
      note = saved.note ?? undefined;
      message.textContent = "Settings saved.";
      message.className = "";
      loadNotes();
    },
    onClose() {
      fitWindow(WINDOW_HEIGHT);
      render();
      editor.focus();
    },
    onResize(height) {
      fitWindow(Math.max(WINDOW_HEIGHT, Math.ceil(height)));
    },
  });

  function render() {
    const blocked = busy || dialog.isOpen;
    editor.setReadOnly(blocked || !draftLoaded || savedPath !== undefined);
    checkbox.disabled = blocked || settings === undefined || savedPath !== undefined;
    blockFormat.disabled = checkbox.disabled;
    gear.disabled = blocked || savedPath !== undefined;
    journal.toggleAttribute("inert", dialog.isOpen);
    journal.hidden = dialog.isOpen;
    checkbox.checked = settings?.use_yesterday_if_today_missing ?? false;
    blockFormat.checked = settings?.entry_format === "block";
    const inactive = note?.is_yesterday === true && !checkbox.checked;
    noteName.textContent = note?.name ?? "No File Selected";
    noteName.classList.toggle("inactive", inactive);
    noteName.title = inactive ? "Yesterday's note is used only when the checkbox is ticked." : "";
    root.setAttribute("aria-busy", String(busy));
  }

  // Loads the `[[` suggestions. Failures leave the previous list; they never block typing.
  function loadNotes() {
    if (notesLoading) {
      notesStale = true;
      return;
    }
    notesLoading = true;
    api.listNotes().then(
      (index) => {
        notes = index.notes;
        if (index.problem) console.warn("note index:", index.problem);
      },
      (error) => console.warn("note index failed", error),
    ).finally(() => {
      notesLoading = false;
      notesLoadedAt = Date.now();
      if (disposed) return;
      if (notesStale) {
        notesStale = false;
        loadNotes();
      } else {
        refreshLinkCompletion(editor);
      }
    });
  }

  async function reloadSettings() {
    try {
      settings = await api.loadSettings();
      note = settings.note ?? undefined;
    } catch (error) {
      note = undefined;
      throw error;
    }
  }

  // Configuration problems are fixed and explained in the Settings dialog.
  function displayMessage(error: unknown) {
    return isConfigError(error) ? "Config error!" : errorMessage(error);
  }

  function showError(error: unknown) {
    message.textContent = displayMessage(error);
    message.className = "error";
  }

  async function close() {
    busy = true;
    render();
    await autosave.idle();
    try {
      await api.exit(savedPath === undefined && draftLoaded ? editor.value() : undefined);
    } catch (error) {
      showError(
        savedPath === undefined
          ? error
          : `Saved to ${savedPath}. Could not close: ${errorMessage(error)} Press Escape to close.`,
      );
      busy = false;
      render();
      editor.focus();
    }
  }

  async function restoreDraft() {
    editor.setValue(await api.loadDraft());
    autosave.reset(editor.value());
    draftLoaded = true;
    editor.moveCursorToEnd();
  }

  async function save() {
    if (busy || dialog.isOpen || savedPath !== undefined) return;
    busy = true;
    message.textContent = "Saving...";
    message.className = "";
    render();
    await autosave.idle();
    try {
      if (!draftLoaded) {
        await restoreDraft();
        message.textContent = "Draft loaded. Press Enter to save or Escape to keep it for later.";
        busy = false;
        render();
        editor.focus();
        return;
      }
      await reloadSettings();
      const result = await api.submit(editor.value());
      savedPath = result.note_path;
    } catch (error) {
      showError(error);
      busy = false;
      render();
      editor.focus();
      return;
    }
    message.textContent = `Saved to ${savedPath}. Closing...`;
    await close();
  }

  async function changeFallback() {
    const enabled = checkbox.checked;
    await changeSetting(() => api.setFallback(enabled));
  }

  async function changeEntryFormat() {
    const format = blockFormat.checked ? "block" : "inline";
    await changeSetting(() => api.setEntryFormat(format));
  }

  // Callers read the new value before this runs, because render() restores both checkboxes.
  async function changeSetting(apply: () => Promise<Settings>) {
    if (busy || dialog.isOpen || savedPath !== undefined) return;
    busy = true;
    message.textContent = "";
    render();
    await autosave.idle();
    try {
      settings = await apply();
      note = settings.note ?? undefined;
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
      render();
      editor.focus();
    }
  }

  async function openSettings() {
    if (busy || dialog.isOpen || savedPath !== undefined) return;
    busy = true;
    render();
    await autosave.idle();
    try {
      dialog.show(await api.readSettingsForm());
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
      render();
      if (!dialog.isOpen) editor.focus();
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.isComposing || event.keyCode === 229) return;
    if (dialog.isOpen) {
      if (event.key === "Escape") {
        event.preventDefault();
        dialog.cancel();
      }
      return;
    }
    // The `[[` suggestion list already handled Enter or Escape.
    if (event.defaultPrevented) return;
    if (event.key === "Escape") {
      event.preventDefault();
      if (!busy && !event.repeat) void close();
    } else if (event.key === "Enter" && event.target === entry && !event.shiftKey) {
      event.preventDefault();
      if (!event.repeat && !busy && savedPath === undefined) {
        if (!draftLoaded || !continueList(editor)) void save();
      }
    }
  }

  async function startDragging(event: PointerEvent) {
    if (event.button !== 0) return;
    try {
      await api.startDragging();
    } catch (error) {
      showError(error);
    }
  }

  const onBlur = () => void autosave.flush();
  const onCloseRequested = () => {
    if (!busy) void close();
  };
  let unlistenClose: (() => void) | undefined;
  api.onCloseRequested(onCloseRequested).then(
    (unlisten) => {
      if (disposed) unlisten();
      else unlistenClose = unlisten;
    },
    (error) => showError(error),
  );

  window.addEventListener("blur", onBlur);
  checkbox.addEventListener("change", changeFallback);
  blockFormat.addEventListener("change", changeEntryFormat);
  gear.addEventListener("click", openSettings);
  dragArea.addEventListener("pointerdown", startDragging);
  window.addEventListener("keydown", onKeyDown);
  render();
  editor.focus();
  async function initialize() {
    const errors: string[] = [];
    try {
      await restoreDraft();
    } catch (error) {
      errors.push(`${errorMessage(error)} Press Enter to retry loading it.`);
    }
    try {
      await reloadSettings();
      if (settings?.use_yesterday_if_today_missing && note !== undefined && !note.is_yesterday) {
        settings = await api.setFallback(false);
        note = settings.note ?? undefined;
      }
    } catch (error) {
      errors.push(displayMessage(error));
    }
    if (errors.length) showError(errors.join("\n"));
    busy = false;
    render();
    editor.focus();
    loadNotes();
  }
  void initialize();

  return () => {
    disposed = true;
    unlistenClose?.();
    autosave.dispose();
    clearTimeout(noticeTimer);
    window.removeEventListener("blur", onBlur);
    window.removeEventListener("keydown", onKeyDown);
    checkbox.removeEventListener("change", changeFallback);
    blockFormat.removeEventListener("change", changeEntryFormat);
    gear.removeEventListener("click", openSettings);
    dialog.destroy();
    editor.destroy();
    dragArea.removeEventListener("pointerdown", startDragging);
  };
}
