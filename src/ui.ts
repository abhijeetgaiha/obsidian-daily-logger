import { errorMessage, type JournalApi, type Settings } from "./api";

export function mountJournal(root: HTMLElement, api: JournalApi): () => void {
  root.innerHTML = `
    <div class="drag-area" aria-hidden="true"></div>
    <textarea id="entry" aria-label="Journal entry"
      placeholder="Write a journal entry..." spellcheck="true"
      aria-describedby="message"></textarea>
    <label class="fallback">
      <input id="fallback" type="checkbox" />
      Use yesterday if today is missing
    </label>
    <p id="message" role="status" aria-live="polite"></p>
  `;
  const entry = root.querySelector<HTMLTextAreaElement>("#entry")!;
  const checkbox = root.querySelector<HTMLInputElement>("#fallback")!;
  const message = root.querySelector<HTMLParagraphElement>("#message")!;
  const dragArea = root.querySelector<HTMLDivElement>(".drag-area")!;
  let settings: Settings | undefined;
  let busy = true;
  let savedPath: string | undefined;

  function render() {
    entry.readOnly = busy || savedPath !== undefined;
    checkbox.disabled = busy || settings === undefined || savedPath !== undefined;
    checkbox.checked = settings?.use_yesterday_if_today_missing ?? false;
    root.setAttribute("aria-busy", String(busy));
  }

  function showError(error: unknown) {
    message.textContent = errorMessage(error);
    message.className = "error";
  }

  async function close() {
    busy = true;
    render();
    try {
      await api.exit();
    } catch (error) {
      showError(
        savedPath === undefined
          ? error
          : `Saved to ${savedPath}. Could not close: ${errorMessage(error)} Press Escape to close.`,
      );
      busy = false;
      render();
    }
  }

  async function save() {
    if (busy || savedPath !== undefined) return;
    busy = true;
    message.textContent = "Saving...";
    message.className = "";
    render();
    try {
      settings = await api.loadSettings();
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
    if (busy || savedPath !== undefined) return;
    const enabled = checkbox.checked;
    busy = true;
    message.textContent = "";
    render();
    try {
      settings = await api.setFallback(enabled);
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
      render();
      entry.focus();
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.isComposing || event.keyCode === 229) return;
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

  checkbox.addEventListener("change", changeFallback);
  dragArea.addEventListener("pointerdown", startDragging);
  window.addEventListener("keydown", onKeyDown);
  render();
  entry.focus();
  void api.loadSettings().then(
    (loaded) => {
      settings = loaded;
    },
    showError,
  ).finally(() => {
    busy = false;
    render();
    entry.focus();
  });

  return () => {
    window.removeEventListener("keydown", onKeyDown);
    checkbox.removeEventListener("change", changeFallback);
    dragArea.removeEventListener("pointerdown", startDragging);
  };
}
