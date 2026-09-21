import { afterEach, describe, expect, it, vi } from "vitest";
import { errorMessage, type JournalApi, type SavedEntry, type Settings } from "./api";
import { mountJournal } from "./ui";

const initial: Settings = {
  config_path: "test-config.json",
  use_yesterday_if_today_missing: false,
};

function mockApi() {
  return {
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

  it("Escape exits without submitting and ignores repeated Escape", async () => {
    const { api, entry } = await setup();
    key(entry, "Escape", { repeat: true });
    expect(api.exit).not.toHaveBeenCalled();
    key(entry, "Escape");
    await flush();
    expect(api.exit).toHaveBeenCalledOnce();
    expect(api.submit).not.toHaveBeenCalled();
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
  });

  it("formats unknown errors explicitly", () => {
    expect(errorMessage("Disconnected")).toBe("Disconnected");
    expect(errorMessage(null)).toContain("Unexpected application error");
  });
});
