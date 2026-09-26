import { errorMessage, type JournalApi, type NoteLabel, type Settings } from "./api";
import { createAutosave } from "./autosave";
import { mountSettingsDialog } from "./settings";

const AUTOSAVE_NOTICE_MS = 1500;

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
      <textarea id="entry" aria-label="Journal entry"
        placeholder="Write a journal entry..." spellcheck="true"
        aria-describedby="message"></textarea>
      <div class="status-row">
        <label class="fallback">
          <input id="fallback" type="checkbox" />
          Use yesterday if today is missing
        </label>
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
  const entry = root.querySelector<HTMLTextAreaElement>("#entry")!;
  const checkbox = root.querySelector<HTMLInputElement>("#fallback")!;
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
  const autosave = createAutosave({
    read: () => entry.value,
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
  const dialog = mountSettingsDialog(root, api, {
    onSaved(saved) {
      settings = saved;
      note = saved.note ?? undefined;
      message.textContent = "Settings saved.";
      message.className = "";
    },
    onClose() {
      render();
      entry.focus();
    },
  });

  function render() {
    const blocked = busy || dialog.isOpen;
    entry.readOnly = blocked || !draftLoaded || savedPath !== undefined;
    checkbox.disabled = blocked || settings === undefined || savedPath !== undefined;
    gear.disabled = blocked || savedPath !== undefined;
    journal.toggleAttribute("inert", dialog.isOpen);
    checkbox.checked = settings?.use_yesterday_if_today_missing ?? false;
    const inactive = note?.is_yesterday === true && !checkbox.checked;
    noteName.textContent = note?.name ?? "No File Selected";
    noteName.classList.toggle("inactive", inactive);
    noteName.title = inactive ? "Yesterday's note is used only when the checkbox is ticked." : "";
    root.setAttribute("aria-busy", String(busy));
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

  function showError(error: unknown) {
    message.textContent = errorMessage(error);
    message.className = "error";
  }

  async function close() {
    busy = true;
    render();
    await autosave.idle();
    try {
      await api.exit(savedPath === undefined && draftLoaded ? entry.value : undefined);
    } catch (error) {
      showError(
        savedPath === undefined
          ? error
          : `Saved to ${savedPath}. Could not close: ${errorMessage(error)} Press Escape to close.`,
      );
      busy = false;
      render();
      entry.focus();
    }
  }

  async function restoreDraft() {
    entry.value = await api.loadDraft();
    autosave.reset(entry.value);
    draftLoaded = true;
    entry.setSelectionRange(entry.value.length, entry.value.length);
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
        entry.focus();
        return;
      }
      await reloadSettings();
      const result = await api.submit(entry.value);
      savedPath = result.note_path;
    } catch (error) {
      showError(error);
      busy = false;
      render();
      entry.focus();
      return;
    }
    message.textContent = `Saved to ${savedPath}. Closing...`;
    await close();
  }

  async function changeFallback() {
    if (busy || dialog.isOpen || savedPath !== undefined) return;
    const enabled = checkbox.checked;
    busy = true;
    message.textContent = "";
    render();
    await autosave.idle();
    try {
      settings = await api.setFallback(enabled);
      note = settings.note ?? undefined;
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
      render();
      entry.focus();
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
      if (!dialog.isOpen) entry.focus();
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
    if (event.key === "Escape") {
      event.preventDefault();
      if (!busy && !event.repeat) void close();
    } else if (event.key === "Enter" && event.target === entry && !event.shiftKey) {
      event.preventDefault();
      if (!event.repeat) void save();
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

  const onInput = () => autosave.schedule();
  const onBlur = () => void autosave.flush();
  const onCloseRequested = () => {
    if (!busy) void close();
  };
  let disposed = false;
  let unlistenClose: (() => void) | undefined;
  api.onCloseRequested(onCloseRequested).then(
    (unlisten) => {
      if (disposed) unlisten();
      else unlistenClose = unlisten;
    },
    (error) => showError(error),
  );

  entry.addEventListener("input", onInput);
  window.addEventListener("blur", onBlur);
  checkbox.addEventListener("change", changeFallback);
  gear.addEventListener("click", openSettings);
  dragArea.addEventListener("pointerdown", startDragging);
  window.addEventListener("keydown", onKeyDown);
  render();
  entry.focus();
  async function initialize() {
    const errors: string[] = [];
    try {
      await restoreDraft();
    } catch (error) {
      errors.push(`${errorMessage(error)} Press Enter to retry loading it.`);
    }
    try {
      await reloadSettings();
    } catch (error) {
      errors.push(errorMessage(error));
    }
    if (errors.length) showError(errors.join("\n"));
    busy = false;
    render();
    entry.focus();
  }
  void initialize();

  return () => {
    disposed = true;
    unlistenClose?.();
    autosave.dispose();
    clearTimeout(noticeTimer);
    entry.removeEventListener("input", onInput);
    window.removeEventListener("blur", onBlur);
    window.removeEventListener("keydown", onKeyDown);
    checkbox.removeEventListener("change", changeFallback);
    gear.removeEventListener("click", openSettings);
    dialog.destroy();
    dragArea.removeEventListener("pointerdown", startDragging);
  };
}
