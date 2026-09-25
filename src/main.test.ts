import { afterEach, describe, expect, it, vi } from "vitest";
import { errorMessage, type JournalApi, type SavedEntry, type Settings } from "./api";
import { mountJournal } from "./ui";

const initial: Settings = {
  config_path: "test-config.json",
  use_yesterday_if_today_missing: false,
  note: { name: "2026-09-26", is_yesterday: false },
};

function mockApi() {
  return {
    loadDraft: vi.fn<JournalApi["loadDraft"]>().mockResolvedValue(""),
    loadSettings: vi.fn<JournalApi["loadSettings"]>().mockResolvedValue(initial),
    setFallback: vi.fn<JournalApi["setFallback"]>().mockImplementation(async (enabled) => ({
      ...initial, use_yesterday_if_today_missing: enabled,
    })),
    submit: vi.fn<JournalApi["submit"]>().mockResolvedValue({ note_path: "today.md" }),
    exit: vi.fn<JournalApi["exit"]>().mockResolvedValue(undefined),
    startDragging: vi.fn<JournalApi["startDragging"]>().mockResolvedValue(undefined),
  };
}

const cleanups: (() => void)[] = [];
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup());
  document.body.innerHTML = "";
});

async function flush() {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

async function setup(api = mockApi()) {
  const root = document.createElement("main");
  document.body.append(root);
  cleanups.push(mountJournal(root, api));
  await flush();
  return {
    api,
    entry: root.querySelector<HTMLTextAreaElement>("#entry")!,
    checkbox: root.querySelector<HTMLInputElement>("#fallback")!,
    message: root.querySelector<HTMLParagraphElement>("#message")!,
    noteName: root.querySelector<HTMLSpanElement>("#note-name")!,
  };
}

function key(target: HTMLElement, key: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", {
    key, bubbles: true, cancelable: true, ...options,
  });
  target.dispatchEvent(event);
  return event;
}

describe("minimal journal window", () => {
  it("focuses the draft, submits original multiline input, and exits on success", async () => {
    const { api, entry } = await setup();
    expect(document.activeElement).toBe(entry);
    entry.value = "  first\nsecond  ";
    expect(key(entry, "Enter").defaultPrevented).toBe(true);
    await flush();
    expect(api.submit).toHaveBeenCalledExactlyOnceWith("  first\nsecond  ");
    expect(api.exit).toHaveBeenCalledOnce();
    expect(api.exit).toHaveBeenCalledWith(undefined);
    expect(entry.readOnly).toBe(true);
  });

  it("leaves Shift+Enter and IME input alone and ignores repeated Enter", async () => {
    const { api, entry } = await setup();
    expect(key(entry, "Enter", { shiftKey: true }).defaultPrevented).toBe(false);
    expect(key(entry, "Enter", { isComposing: true }).defaultPrevented).toBe(false);
    key(entry, "Enter", { repeat: true });
    await flush();
    expect(api.submit).not.toHaveBeenCalled();
  });

  it("Escape persists exact draft text without submitting and ignores repeated Escape", async () => {
    const { api, entry } = await setup();
    entry.value = "  unfinished\ntext  ";
    key(entry, "Escape", { repeat: true });
    expect(api.exit).not.toHaveBeenCalled();
    key(entry, "Escape");
    await flush();
    expect(api.exit).toHaveBeenCalledOnce();
    expect(api.exit).toHaveBeenCalledWith("  unfinished\ntext  ");
    expect(api.submit).not.toHaveBeenCalled();
  });

  it("restores the draft even if vault configuration is invalid", async () => {
    const api = mockApi();
    api.loadDraft.mockResolvedValue("  pending\nentry  ");
    api.loadSettings.mockRejectedValue(new Error("Fix configuration"));
    const { entry, message } = await setup(api);
    expect(entry.value).toBe("  pending\nentry  ");
    expect(entry.readOnly).toBe(false);
    expect(entry.selectionStart).toBe(entry.value.length);
    expect(message.textContent).toBe("Fix configuration");
    key(entry, "Escape");
    await flush();
    expect(api.exit).toHaveBeenCalledWith("  pending\nentry  ");
  });

  it("keeps the draft editable when Escape cannot persist it", async () => {
    const api = mockApi();
    api.exit.mockRejectedValue(new Error("Could not save draft"));
    const { entry, message } = await setup(api);
    entry.value = "keep this";
    key(entry, "Escape");
    await flush();
    expect(entry.value).toBe("keep this");
    expect(entry.readOnly).toBe(false);
    expect(message.textContent).toBe("Could not save draft");
    api.exit.mockResolvedValue(undefined);
    key(entry, "Escape");
    await flush();
    expect(api.exit).toHaveBeenCalledTimes(2);
  });

  it("clears a previously saved draft when an empty text box is dismissed", async () => {
    const api = mockApi();
    api.loadDraft.mockResolvedValue("old draft");
    const { entry } = await setup(api);
    entry.value = "";
    key(entry, "Escape");
    await flush();
    expect(api.exit).toHaveBeenCalledWith("");
  });

  it("does not overwrite an unreadable draft on Escape", async () => {
    const api = mockApi();
    api.loadDraft.mockRejectedValue(new Error("Cannot read draft"));
    const { entry, message } = await setup(api);
    expect(entry.readOnly).toBe(true);
    expect(message.textContent).toContain("Cannot read draft");
    key(entry, "Escape");
    await flush();
    expect(api.exit).toHaveBeenCalledWith(undefined);
    expect(api.submit).not.toHaveBeenCalled();
  });

  it("allows retrying a failed draft load without submitting it immediately", async () => {
    const api = mockApi();
    api.loadDraft.mockRejectedValue(new Error("Cannot read draft"));
    const { entry } = await setup(api);
    api.loadDraft.mockResolvedValue("recovered");
    key(entry, "Enter");
    await flush();
    expect(entry.value).toBe("recovered");
    expect(entry.readOnly).toBe(false);
    expect(api.submit).not.toHaveBeenCalled();
    key(entry, "Enter");
    await flush();
    expect(api.submit).toHaveBeenCalledExactlyOnceWith("recovered");
  });

  it("retains the draft and displays IPC errors as text", async () => {
    const api = mockApi();
    api.submit.mockRejectedValue({ code: "today_missing", message: "<b>Missing note</b>" });
    const { entry, message } = await setup(api);
    entry.value = "my draft";
    key(entry, "Enter");
    await flush();
    expect(message.textContent).toBe("<b>Missing note</b>");
    expect(message.children).toHaveLength(0);
    expect(entry.value).toBe("my draft");
    expect(entry.readOnly).toBe(false);
    expect(api.exit).not.toHaveBeenCalled();
  });

  it("blocks repeated submits, preference changes, and Escape during a write", async () => {
    const api = mockApi();
    let resolve!: (result: SavedEntry) => void;
    api.submit.mockReturnValue(new Promise((done) => { resolve = done; }));
    const { entry, checkbox } = await setup(api);
    key(entry, "Enter");
    await flush();
    key(entry, "Enter");
    key(entry, "Escape");
    expect(checkbox.disabled).toBe(true);
    expect(api.submit).toHaveBeenCalledOnce();
    expect(api.exit).not.toHaveBeenCalled();
    resolve({ note_path: "today.md" });
    await flush();
    expect(api.exit).toHaveBeenCalledOnce();
  });

  it("persists a changed checkbox and restores it in a new window", async () => {
    const api = mockApi();
    const { checkbox } = await setup(api);
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event("change"));
    await flush();
    expect(api.setFallback).toHaveBeenCalledExactlyOnceWith(true);
    expect(checkbox.checked).toBe(true);
    cleanups.pop()?.();
    document.body.replaceChildren();
    api.loadSettings.mockResolvedValue({ ...initial, use_yesterday_if_today_missing: true });
    expect((await setup(api)).checkbox.checked).toBe(true);
  });

  it("rolls back a failed preference write and retains the draft", async () => {
    const api = mockApi();
    api.setFallback.mockRejectedValue(new Error("Read-only config"));
    const { checkbox, entry, message } = await setup(api);
    entry.value = "draft";
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event("change"));
    await flush();
    expect(checkbox.checked).toBe(false);
    expect(entry.value).toBe("draft");
    expect(message.textContent).toBe("Read-only config");
  });

  it("does not race a pending preference write with a save", async () => {
    const api = mockApi();
    let resolve!: (settings: Settings) => void;
    api.setFallback.mockReturnValue(new Promise((done) => { resolve = done; }));
    const { checkbox, entry } = await setup(api);
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event("change"));
    key(entry, "Enter");
    expect(api.submit).not.toHaveBeenCalled();
    resolve({ ...initial, use_yesterday_if_today_missing: true });
    await flush();
    expect(checkbox.checked).toBe(true);
  });

  it("blocks logging with bad configuration and recovers without losing the draft", async () => {
    const api = mockApi();
    api.loadSettings.mockRejectedValue({ message: "Fix config.json" });
    const { entry, checkbox, message } = await setup(api);
    entry.value = "draft";
    expect(checkbox.disabled).toBe(true);
    key(entry, "Enter");
    await flush();
    expect(message.textContent).toBe("Fix config.json");
    expect(api.submit).not.toHaveBeenCalled();
    expect(entry.value).toBe("draft");
    api.loadSettings.mockResolvedValue(initial);
    key(entry, "Enter");
    await flush();
    expect(api.submit).toHaveBeenCalledExactlyOnceWith("draft");
  });

  it("never resubmits after a successful save if exiting fails", async () => {
    const api = mockApi();
    api.exit.mockRejectedValue(new Error("Cannot exit"));
    const { entry, message } = await setup(api);
    key(entry, "Enter");
    await flush();
    expect(message.textContent).toContain("Saved to today.md");
    expect(message.textContent).toContain("Cannot exit");
    key(entry, "Enter");
    await flush();
    expect(api.submit).toHaveBeenCalledOnce();
    api.exit.mockResolvedValue(undefined);
    key(entry, "Escape");
    await flush();
    expect(api.exit).toHaveBeenCalledTimes(2);
    expect(api.exit).toHaveBeenLastCalledWith(undefined);
  });

  it("shows today's note name in the status row", async () => {
    const { noteName } = await setup();
    expect(noteName.textContent).toBe("2026-09-26");
    expect(noteName.classList.contains("inactive")).toBe(false);
    expect(noteName.title).toBe("");
  });

  it("shows yesterday's note dimmed until the checkbox is ticked", async () => {
    const api = mockApi();
    const yesterday = { name: "2026-09-25", is_yesterday: true };
    api.loadSettings.mockResolvedValue({ ...initial, note: yesterday });
    api.setFallback.mockImplementation(async (enabled) => ({
      ...initial, use_yesterday_if_today_missing: enabled, note: yesterday,
    }));
    const { checkbox, noteName } = await setup(api);
    expect(noteName.textContent).toBe("2026-09-25");
    expect(noteName.classList.contains("inactive")).toBe(true);
    expect(noteName.title).toContain("checkbox");
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event("change"));
    await flush();
    expect(noteName.textContent).toBe("2026-09-25");
    expect(noteName.classList.contains("inactive")).toBe(false);
    expect(noteName.title).toBe("");
  });

  it("shows No File Selected when no note or configuration is available", async () => {
    const api = mockApi();
    api.loadSettings.mockResolvedValue({ ...initial, note: null });
    expect((await setup(api)).noteName.textContent).toBe("No File Selected");
    cleanups.pop()?.();
    document.body.replaceChildren();
    api.loadSettings.mockRejectedValue(new Error("Fix configuration"));
    const { noteName } = await setup(api);
    expect(noteName.textContent).toBe("No File Selected");
    expect(noteName.classList.contains("inactive")).toBe(false);
  });

  it("refreshes the note name on each save attempt", async () => {
    const api = mockApi();
    api.submit.mockRejectedValue(new Error("Missing note"));
    const { entry, noteName } = await setup(api);
    entry.value = "draft";
    api.loadSettings.mockResolvedValue({
      ...initial, note: { name: "2026-09-27", is_yesterday: false },
    });
    key(entry, "Enter");
    await flush();
    expect(noteName.textContent).toBe("2026-09-27");
    api.loadSettings.mockRejectedValue(new Error("Fix config.json"));
    key(entry, "Enter");
    await flush();
    expect(noteName.textContent).toBe("No File Selected");
  });

  it("renders the note name as text", async () => {
    const api = mockApi();
    api.loadSettings.mockResolvedValue({
      ...initial, note: { name: "<b>note</b>", is_yesterday: false },
    });
    const { noteName } = await setup(api);
    expect(noteName.textContent).toBe("<b>note</b>");
    expect(noteName.children).toHaveLength(0);
  });

  it("formats unknown errors explicitly", () => {
    expect(errorMessage("Disconnected")).toBe("Disconnected");
    expect(errorMessage(null)).toContain("Unexpected application error");
  });
});
