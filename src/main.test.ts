import { afterEach, describe, expect, it, vi } from "vitest";
import { AUTOSAVE_DELAY_MS, AUTOSAVE_MAX_WAIT_MS } from "./autosave";
import {
  errorMessage, type FormResult, type HeadingList, type JournalApi, type SavedEntry,
  type Settings,
} from "./api";
import { mountJournal } from "./ui";

const initial: Settings = {
  config_path: "test-config.json",
  use_yesterday_if_today_missing: false,
  entry_format: "inline",
  note: { name: "2026-09-26", is_yesterday: false },
};

const noHeading = { heading: "", duplicate_heading: "error", entry_format: "inline" } as const;

const validForm: FormResult = {
  config_path: "/config/config.json",
  exists: true,
  form: { vault_root: "/vault", ...noHeading, use_yesterday_if_today_missing: false },
  issue: null,
};

let closeRequested: (() => void) | undefined;
const unlistenClose = vi.fn();

const noteHeadings: HeadingList = {
  note: "2026-09-26",
  headings: ["# Journal", "## Daily Log"],
  problem: null,
};

function mockApi() {
  return {
    loadDraft: vi.fn<JournalApi["loadDraft"]>().mockResolvedValue(""),
    saveDraft: vi.fn<JournalApi["saveDraft"]>().mockResolvedValue(undefined),
    onCloseRequested: vi.fn<JournalApi["onCloseRequested"]>().mockImplementation(
      async (handler) => {
        closeRequested = handler;
        return unlistenClose;
      },
    ),
    loadSettings: vi.fn<JournalApi["loadSettings"]>().mockResolvedValue(initial),
    setFallback: vi.fn<JournalApi["setFallback"]>().mockImplementation(async (enabled) => ({
      ...initial, use_yesterday_if_today_missing: enabled,
    })),
    setEntryFormat: vi.fn<JournalApi["setEntryFormat"]>().mockImplementation(async (format) => ({
      ...initial, entry_format: format,
    })),
    readSettingsForm: vi.fn<JournalApi["readSettingsForm"]>().mockResolvedValue(validForm),
    listHeadings: vi.fn<JournalApi["listHeadings"]>().mockResolvedValue(noteHeadings),
    pickVaultFolder: vi.fn<JournalApi["pickVaultFolder"]>().mockResolvedValue("/picked"),
    saveSettings: vi.fn<JournalApi["saveSettings"]>().mockImplementation(async (form) => ({
      ...initial, use_yesterday_if_today_missing: form.use_yesterday_if_today_missing,
      entry_format: form.entry_format,
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
  closeRequested = undefined;
  vi.useRealTimers();
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
    blockFormat: root.querySelector<HTMLInputElement>("#block-format")!,
    message: root.querySelector<HTMLParagraphElement>("#message")!,
    noteName: root.querySelector<HTMLSpanElement>("#note-name")!,
    gear: root.querySelector<HTMLButtonElement>("#settings")!,
    journal: root.querySelector<HTMLDivElement>("#journal")!,
    dialog: root.querySelector<HTMLDivElement>("#settings-dialog")!,
    issue: root.querySelector<HTMLParagraphElement>("#settings-issue")!,
    folder: root.querySelector<HTMLSpanElement>("#settings-folder")!,
    choose: root.querySelector<HTMLButtonElement>("#settings-choose")!,
    dialogFallback: root.querySelector<HTMLInputElement>("#settings-fallback")!,
    heading: root.querySelector<HTMLSelectElement>("#settings-heading")!,
    headingNote: root.querySelector<HTMLParagraphElement>("#settings-heading-note")!,
    duplicates: root.querySelector<HTMLSelectElement>("#settings-duplicates")!,
    entryFormat: root.querySelector<HTMLSelectElement>("#settings-entry-format")!,
    dialogMessage: root.querySelector<HTMLParagraphElement>("#settings-message")!,
    saveButton: root.querySelector<HTMLButtonElement>("#settings-save")!,
    cancelButton: root.querySelector<HTMLButtonElement>("#settings-cancel")!,
    notice: root.querySelector<HTMLSpanElement>("#autosave")!,
  };
}

function type(entry: HTMLTextAreaElement, value: string) {
  entry.value = value;
  entry.dispatchEvent(new Event("input"));
}

async function tick(ms = 0) {
  await vi.advanceTimersByTimeAsync(ms);
}

async function setupWithFakeTimers(api = mockApi()) {
  const view = await setup(api);
  vi.useFakeTimers();
  return view;
}

async function openSettings(api = mockApi()) {
  const view = await setup(api);
  view.gear.click();
  await flush();
  return view;
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
    const yesterday = { name: "2026-09-25", is_yesterday: true };
    api.loadSettings.mockResolvedValue({
      ...initial, use_yesterday_if_today_missing: true, note: yesterday,
    });
    expect((await setup(api)).checkbox.checked).toBe(true);
    expect(api.setFallback).toHaveBeenCalledOnce();
  });

  it("unchecks and saves the fallback at launch when today's note exists", async () => {
    const api = mockApi();
    api.loadSettings.mockResolvedValue({ ...initial, use_yesterday_if_today_missing: true });
    const { checkbox, entry, message } = await setup(api);
    expect(api.setFallback).toHaveBeenCalledExactlyOnceWith(false);
    expect(checkbox.checked).toBe(false);
    expect(checkbox.disabled).toBe(false);
    expect(message.textContent).toBe("");
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event("change"));
    await flush();
    expect(api.setFallback).toHaveBeenLastCalledWith(true);
    api.submit.mockRejectedValue(new Error("Missing note"));
    entry.value = "draft";
    key(entry, "Enter");
    await flush();
    expect(api.setFallback).toHaveBeenCalledTimes(2);
    expect(checkbox.checked).toBe(true);
  });

  it("leaves the fallback alone at launch without today's note or when it is off", async () => {
    const api = mockApi();
    for (const note of [{ name: "2026-09-25", is_yesterday: true }, null]) {
      api.loadSettings.mockResolvedValue({
        ...initial, use_yesterday_if_today_missing: true, note,
      });
      expect((await setup(api)).checkbox.checked).toBe(true);
      cleanups.pop()?.();
      document.body.replaceChildren();
    }
    api.loadSettings.mockResolvedValue(initial);
    expect((await setup(api)).checkbox.checked).toBe(false);
    expect(api.setFallback).not.toHaveBeenCalled();
  });

  it("shows an error and keeps the saved fallback if unchecking at launch fails", async () => {
    const api = mockApi();
    api.loadSettings.mockResolvedValue({ ...initial, use_yesterday_if_today_missing: true });
    api.setFallback.mockRejectedValue(new Error("Read-only config"));
    const { checkbox, message } = await setup(api);
    expect(checkbox.checked).toBe(true);
    expect(checkbox.disabled).toBe(false);
    expect(message.textContent).toBe("Read-only config");
    expect(message.className).toBe("error");
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

  it("persists the block format toggle and rolls it back on failure", async () => {
    const api = mockApi();
    const { blockFormat, checkbox, entry } = await setup(api);
    expect(blockFormat.checked).toBe(false);
    expect(blockFormat.disabled).toBe(false);
    blockFormat.checked = true;
    blockFormat.dispatchEvent(new Event("change"));
    await flush();
    expect(api.setEntryFormat).toHaveBeenCalledExactlyOnceWith("block");
    expect(api.setFallback).not.toHaveBeenCalled();
    expect(blockFormat.checked).toBe(true);
    expect(checkbox.checked).toBe(false);
    expect(document.activeElement).toBe(entry);
    api.setEntryFormat.mockRejectedValueOnce(new Error("Read-only config"));
    blockFormat.checked = false;
    blockFormat.dispatchEvent(new Event("change"));
    await flush();
    expect(api.setEntryFormat).toHaveBeenLastCalledWith("inline");
    expect(blockFormat.checked).toBe(true);
    cleanups.pop()?.();
    document.body.replaceChildren();
    api.loadSettings.mockResolvedValue({ ...initial, entry_format: "block" });
    expect((await setup(api)).blockFormat.checked).toBe(true);
  });

  it("disables the block format toggle during a write and without settings", async () => {
    const api = mockApi();
    let resolve!: (result: SavedEntry) => void;
    api.submit.mockReturnValue(new Promise((done) => { resolve = done; }));
    const { entry, blockFormat } = await setup(api);
    key(entry, "Enter");
    await flush();
    expect(blockFormat.disabled).toBe(true);
    resolve({ note_path: "today.md" });
    await flush();
    expect(blockFormat.disabled).toBe(true);
    cleanups.pop()?.();
    document.body.replaceChildren();
    api.loadSettings.mockRejectedValue(new Error("bad config"));
    expect((await setup(api)).blockFormat.disabled).toBe(true);
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

  it("places the settings gear right after the note name", async () => {
    const { gear, noteName, dialog } = await setup();
    expect(noteName.nextElementSibling).toBe(gear);
    expect(gear.getAttribute("aria-label")).toBe("Settings");
    expect(gear.disabled).toBe(false);
    expect(dialog.hidden).toBe(true);
  });

  it("opens the dialog with the existing settings loaded", async () => {
    const api = mockApi();
    api.readSettingsForm.mockResolvedValue({
      ...validForm, form: {
        vault_root: "/vault", heading: "## Daily Log", duplicate_heading: "last",
        entry_format: "block", use_yesterday_if_today_missing: true,
      },
    });
    const { dialog, folder, dialogFallback, issue, saveButton, journal, entry, choose,
      entryFormat } = await openSettings(api);
    expect(api.readSettingsForm).toHaveBeenCalledOnce();
    expect(dialog.hidden).toBe(false);
    expect(folder.textContent).toBe("/vault");
    expect(dialogFallback.checked).toBe(true);
    expect(entryFormat.value).toBe("block");
    expect(issue.textContent).toBe("");
    expect(saveButton.disabled).toBe(false);
    expect(journal.hasAttribute("inert")).toBe(true);
    expect(entry.readOnly).toBe(true);
    expect(document.activeElement).toBe(choose);
  });

  it("opens an empty form when no settings file exists and creates it on save", async () => {
    const api = mockApi();
    api.readSettingsForm.mockResolvedValue({
      config_path: "/config/config.json",
      exists: false,
      form: { vault_root: null, ...noHeading, use_yesterday_if_today_missing: false },
      issue: null,
    });
    const { folder, issue, saveButton, choose, dialogFallback, dialog, checkbox, message } =
      await openSettings(api);
    expect(folder.textContent).toBe("Not set");
    expect(issue.textContent).toContain("Saving creates /config/config.json");
    expect(issue.className).toBe("");
    expect(saveButton.disabled).toBe(true);
    choose.click();
    await flush();
    expect(api.pickVaultFolder).toHaveBeenCalledExactlyOnceWith(null);
    expect(folder.textContent).toBe("/picked");
    dialogFallback.checked = true;
    dialogFallback.dispatchEvent(new Event("change"));
    saveButton.click();
    await flush();
    expect(api.saveSettings).toHaveBeenCalledExactlyOnceWith({
      vault_root: "/picked", ...noHeading, use_yesterday_if_today_missing: true,
    });
    expect(dialog.hidden).toBe(true);
    expect(checkbox.checked).toBe(true);
    expect(checkbox.disabled).toBe(false);
    expect(message.textContent).toBe("Settings saved.");
  });

  it("shows partially valid settings with the problem and allows replacing them", async () => {
    const api = mockApi();
    api.readSettingsForm.mockResolvedValue({
      ...validForm,
      form: { vault_root: "/gone", ...noHeading, use_yesterday_if_today_missing: false },
      issue: "<b>vault_root must be an existing directory</b>",
    });
    const { folder, issue, saveButton, choose } = await openSettings(api);
    expect(folder.textContent).toBe("/gone");
    expect(issue.textContent).toBe("<b>vault_root must be an existing directory</b>");
    expect(issue.children).toHaveLength(0);
    expect(issue.className).toBe("error");
    choose.click();
    await flush();
    expect(api.pickVaultFolder).toHaveBeenCalledWith("/gone");
    saveButton.click();
    await flush();
    expect(api.saveSettings).toHaveBeenCalledWith({
      vault_root: "/picked", ...noHeading, use_yesterday_if_today_missing: false,
    });
  });

  it("keeps the current folder when the picker is cancelled or fails", async () => {
    const api = mockApi();
    api.pickVaultFolder.mockResolvedValue(null);
    const { folder, choose, dialogMessage } = await openSettings(api);
    choose.click();
    await flush();
    expect(folder.textContent).toBe("/vault");
    api.pickVaultFolder.mockRejectedValue(new Error("Picker failed"));
    choose.click();
    await flush();
    expect(folder.textContent).toBe("/vault");
    expect(dialogMessage.textContent).toBe("Picker failed");
  });

  it("refreshes the note name after saving settings", async () => {
    const api = mockApi();
    api.saveSettings.mockResolvedValue({
      ...initial, note: { name: "2026-09-25", is_yesterday: true },
    });
    const { saveButton, noteName, entry } = await openSettings(api);
    saveButton.click();
    await flush();
    expect(noteName.textContent).toBe("2026-09-25");
    expect(noteName.classList.contains("inactive")).toBe(true);
    expect(document.activeElement).toBe(entry);
  });

  it("keeps the dialog open and the draft intact when saving settings fails", async () => {
    const api = mockApi();
    api.loadDraft.mockResolvedValue("my draft");
    api.saveSettings.mockRejectedValue({ code: "settings", message: "Cannot save" });
    const { saveButton, dialog, dialogMessage, entry } = await openSettings(api);
    saveButton.click();
    await flush();
    expect(dialog.hidden).toBe(false);
    expect(dialogMessage.textContent).toBe("Cannot save");
    expect(dialogMessage.className).toBe("error");
    expect(saveButton.disabled).toBe(false);
    expect(entry.value).toBe("my draft");
  });

  it("Escape and Cancel close the dialog without exiting or saving", async () => {
    const api = mockApi();
    const { dialog, entry, cancelButton, gear, journal } = await openSettings(api);
    expect(key(dialog, "Escape").defaultPrevented).toBe(true);
    await flush();
    expect(dialog.hidden).toBe(true);
    expect(journal.hasAttribute("inert")).toBe(false);
    expect(document.activeElement).toBe(entry);
    expect(api.exit).not.toHaveBeenCalled();
    gear.click();
    await flush();
    cancelButton.click();
    await flush();
    expect(dialog.hidden).toBe(true);
    expect(api.saveSettings).not.toHaveBeenCalled();
    expect(api.exit).not.toHaveBeenCalled();
    key(entry, "Escape");
    await flush();
    expect(api.exit).toHaveBeenCalledOnce();
  });

  it("does not submit the journal entry on Enter while the dialog is open", async () => {
    const api = mockApi();
    const { entry, dialog } = await openSettings(api);
    entry.value = "draft";
    key(entry, "Enter");
    key(dialog, "Enter");
    await flush();
    expect(api.submit).not.toHaveBeenCalled();
    expect(dialog.hidden).toBe(false);
  });

  it("ignores Escape while a settings operation is pending", async () => {
    const api = mockApi();
    let resolve!: (path: string | null) => void;
    api.pickVaultFolder.mockReturnValue(new Promise((done) => { resolve = done; }));
    const { choose, dialog, saveButton, cancelButton } = await openSettings(api);
    choose.click();
    await flush();
    expect(saveButton.disabled).toBe(true);
    expect(cancelButton.disabled).toBe(true);
    key(dialog, "Escape");
    expect(dialog.hidden).toBe(false);
    resolve("/new");
    await flush();
    expect(cancelButton.disabled).toBe(false);
  });

  it("shows an error instead of the dialog when settings cannot be read", async () => {
    const api = mockApi();
    api.readSettingsForm.mockRejectedValue(new Error("Busy"));
    const { dialog, message, entry } = await openSettings(api);
    expect(dialog.hidden).toBe(true);
    expect(message.textContent).toBe("Busy");
    expect(document.activeElement).toBe(entry);
  });

  it("disables the gear during a save and after an entry is logged", async () => {
    const api = mockApi();
    api.exit.mockRejectedValue(new Error("Cannot exit"));
    let resolve!: (result: SavedEntry) => void;
    api.submit.mockReturnValue(new Promise((done) => { resolve = done; }));
    const { entry, gear } = await setup(api);
    key(entry, "Enter");
    await flush();
    expect(gear.disabled).toBe(true);
    resolve({ note_path: "today.md" });
    await flush();
    expect(gear.disabled).toBe(true);
    gear.click();
    await flush();
    expect(api.readSettingsForm).not.toHaveBeenCalled();
  });

  it("lists End of file and the note's headings, selecting the saved one", async () => {
    const api = mockApi();
    api.readSettingsForm.mockResolvedValue({
      ...validForm,
      form: { ...validForm.form, heading: "## Daily Log", duplicate_heading: "last" },
    });
    const { heading, duplicates, headingNote } = await openSettings(api);
    expect(api.listHeadings).toHaveBeenCalledExactlyOnceWith("/vault");
    expect([...heading.options].map((item) => [item.value, item.textContent])).toEqual([
      ["", "End of file"], ["# Journal", "# Journal"], ["## Daily Log", "## Daily Log"],
    ]);
    expect(heading.value).toBe("## Daily Log");
    expect(headingNote.textContent).toBe("Headings from 2026-09-26.");
    expect(duplicates.value).toBe("last");
    expect(duplicates.disabled).toBe(false);
    heading.value = "";
    heading.dispatchEvent(new Event("change"));
    expect(duplicates.disabled).toBe(true);
  });

  it("falls back to End of file with a notice when the saved heading is missing", async () => {
    const api = mockApi();
    api.readSettingsForm.mockResolvedValue({
      ...validForm, form: { ...validForm.form, heading: "# Gone" },
    });
    const { heading, headingNote, saveButton } = await openSettings(api);
    expect(heading.value).toBe("");
    expect([...heading.options].map((item) => item.value)).not.toContain("# Gone");
    expect(headingNote.textContent).toBe(
      "\"# Gone\" isn't in 2026-09-26; End of file selected.",
    );
    expect(headingNote.className).toBe("notice");
    saveButton.click();
    await flush();
    expect(api.saveSettings).toHaveBeenCalledWith({ ...validForm.form, heading: "" });
  });

  it("offers only End of file without a daily note or folder", async () => {
    const api = mockApi();
    api.listHeadings.mockResolvedValue({ note: null, headings: [], problem: null });
    api.readSettingsForm.mockResolvedValue({
      ...validForm, form: { ...validForm.form, heading: "# Journal" },
    });
    const { heading, headingNote } = await openSettings(api);
    expect([...heading.options].map((item) => item.value)).toEqual([""]);
    expect(heading.value).toBe("");
    expect(headingNote.textContent).toContain("isn't in a daily note");
    cleanups.pop()?.();
    document.body.replaceChildren();
    api.readSettingsForm.mockResolvedValue({
      config_path: "/config/config.json", exists: false,
      form: { vault_root: null, ...noHeading, use_yesterday_if_today_missing: false },
      issue: null,
    });
    const view = await openSettings(api);
    expect(api.listHeadings).toHaveBeenLastCalledWith(null);
    expect(view.headingNote.textContent).toBe("Choose a journal folder to list its headings.");
  });

  it("rescans headings after choosing a different folder", async () => {
    const api = mockApi();
    const { heading, choose, headingNote } = await openSettings(api);
    api.listHeadings.mockResolvedValue({
      note: "2026-09-25", headings: ["## Elsewhere"], problem: null,
    });
    choose.click();
    await flush();
    expect(api.listHeadings).toHaveBeenLastCalledWith("/picked");
    expect([...heading.options].map((item) => item.value)).toEqual(["", "## Elsewhere"]);
    expect(headingNote.textContent).toBe("Headings from 2026-09-25.");
    api.pickVaultFolder.mockResolvedValue(null);
    choose.click();
    await flush();
    expect(api.listHeadings).toHaveBeenCalledTimes(2);
  });

  it("shows listing problems and IPC errors without dropping the saved heading", async () => {
    const api = mockApi();
    api.readSettingsForm.mockResolvedValue({
      ...validForm, form: { ...validForm.form, heading: "# Journal" },
    });
    api.listHeadings.mockResolvedValue({
      note: "2026-09-26", headings: [], problem: "Could not read headings: bad UTF-8",
    });
    const { heading, headingNote } = await openSettings(api);
    expect(heading.value).toBe("# Journal");
    expect(headingNote.textContent).toBe("Could not read headings: bad UTF-8");
    expect(headingNote.className).toBe("error");
    cleanups.pop()?.();
    document.body.replaceChildren();
    api.listHeadings.mockRejectedValue(new Error("Busy"));
    const view = await openSettings(api);
    expect(view.heading.value).toBe("# Journal");
    expect(view.headingNote.textContent).toBe("Could not list headings: Busy");
  });

  it("saves the selected heading and duplicate policy with the other settings", async () => {
    const api = mockApi();
    const { heading, duplicates, saveButton, entry } = await openSettings(api);
    heading.value = "## Daily Log";
    heading.dispatchEvent(new Event("change"));
    duplicates.value = "first";
    duplicates.dispatchEvent(new Event("change"));
    key(heading, "Enter");
    await flush();
    expect(api.submit).not.toHaveBeenCalled();
    saveButton.click();
    await flush();
    expect(api.saveSettings).toHaveBeenCalledExactlyOnceWith({
      vault_root: "/vault", heading: "## Daily Log", duplicate_heading: "first",
      entry_format: "inline", use_yesterday_if_today_missing: false,
    });
    expect(document.activeElement).toBe(entry);
  });

  it("saves the entry format from the dialog and updates the main toggle", async () => {
    const api = mockApi();
    const { entryFormat, saveButton, blockFormat } = await openSettings(api);
    expect(entryFormat.value).toBe("inline");
    expect(blockFormat.checked).toBe(false);
    entryFormat.value = "block";
    entryFormat.dispatchEvent(new Event("change"));
    saveButton.click();
    await flush();
    expect(api.saveSettings).toHaveBeenCalledExactlyOnceWith({
      ...validForm.form, entry_format: "block",
    });
    expect(blockFormat.checked).toBe(true);
    expect(api.setEntryFormat).not.toHaveBeenCalled();
  });

  it("renders note headings as text", async () => {
    const api = mockApi();
    api.listHeadings.mockResolvedValue({
      note: "<i>n</i>", headings: ["# <b>x</b>"], problem: null,
    });
    const { heading, headingNote, dialog } = await openSettings(api);
    expect(heading.options[1].textContent).toBe("# <b>x</b>");
    expect(headingNote.textContent).toBe("Headings from <i>n</i>.");
    expect(dialog.querySelector("b, i")).toBeNull();
  });

  it("formats unknown errors explicitly", () => {
    expect(errorMessage("Disconnected")).toBe("Disconnected");
    expect(errorMessage(null)).toContain("Unexpected application error");
  });
});

describe("draft autosave", () => {
  it("saves 1 s after typing pauses and skips unchanged text", async () => {
    const api = mockApi();
    api.loadDraft.mockResolvedValue("restored");
    const { entry, notice } = await setupWithFakeTimers(api);
    type(entry, "restored");
    await tick(AUTOSAVE_DELAY_MS * 2);
    expect(api.saveDraft).not.toHaveBeenCalled();
    type(entry, "restored and more");
    await tick(AUTOSAVE_DELAY_MS - 1);
    type(entry, "restored and more text");
    await tick(AUTOSAVE_DELAY_MS - 1);
    expect(api.saveDraft).not.toHaveBeenCalled();
    await tick(1);
    expect(api.saveDraft).toHaveBeenCalledExactlyOnceWith("restored and more text");
    expect(notice.textContent).toBe("Draft saved");
    await tick(1500);
    expect(notice.textContent).toBe("");
    entry.dispatchEvent(new Event("input"));
    await tick(AUTOSAVE_DELAY_MS);
    expect(api.saveDraft).toHaveBeenCalledOnce();
  });

  it("saves at least every 10 s during continuous typing", async () => {
    const api = mockApi();
    const { entry } = await setupWithFakeTimers(api);
    for (let elapsed = 0; elapsed < AUTOSAVE_MAX_WAIT_MS; elapsed += 500) {
      type(entry, `text ${elapsed}`);
      await tick(500);
    }
    expect(api.saveDraft).toHaveBeenCalledOnce();
    expect(api.saveDraft).toHaveBeenCalledWith(`text ${AUTOSAVE_MAX_WAIT_MS - 500}`);
  });

  it("autosaves an emptied text box so the old draft is cleared", async () => {
    const api = mockApi();
    api.loadDraft.mockResolvedValue("old");
    const { entry } = await setupWithFakeTimers(api);
    type(entry, "");
    await tick(AUTOSAVE_DELAY_MS);
    expect(api.saveDraft).toHaveBeenCalledExactlyOnceWith("");
  });

  it("flushes immediately when the window loses focus", async () => {
    const api = mockApi();
    const { entry } = await setupWithFakeTimers(api);
    type(entry, "focus lost");
    window.dispatchEvent(new Event("blur"));
    await tick();
    expect(api.saveDraft).toHaveBeenCalledExactlyOnceWith("focus lost");
    await tick(AUTOSAVE_MAX_WAIT_MS);
    expect(api.saveDraft).toHaveBeenCalledOnce();
  });

  it("never autosaves over a draft that could not be loaded", async () => {
    const api = mockApi();
    api.loadDraft.mockRejectedValue(new Error("Cannot read draft"));
    const { entry } = await setupWithFakeTimers(api);
    type(entry, "typed anyway");
    window.dispatchEvent(new Event("blur"));
    await tick(AUTOSAVE_MAX_WAIT_MS);
    expect(api.saveDraft).not.toHaveBeenCalled();
  });

  it("stops autosaving once the entry is logged", async () => {
    const api = mockApi();
    api.exit.mockRejectedValue(new Error("Cannot exit"));
    const { entry } = await setupWithFakeTimers(api);
    entry.value = "logged";
    key(entry, "Enter");
    await tick();
    expect(api.submit).toHaveBeenCalledOnce();
    type(entry, "logged");
    entry.dispatchEvent(new Event("input"));
    window.dispatchEvent(new Event("blur"));
    await tick(AUTOSAVE_MAX_WAIT_MS);
    expect(api.saveDraft).not.toHaveBeenCalled();
  });

  it("waits while the settings dialog is open and saves after it closes", async () => {
    const api = mockApi();
    const { entry, gear, cancelButton } = await setupWithFakeTimers(api);
    type(entry, "before settings");
    gear.click();
    await tick();
    await tick(AUTOSAVE_DELAY_MS * 3);
    expect(api.saveDraft).not.toHaveBeenCalled();
    cancelButton.click();
    await tick(AUTOSAVE_DELAY_MS);
    expect(api.saveDraft).toHaveBeenCalledExactlyOnceWith("before settings");
  });

  it("Escape and Enter wait for an in-flight autosave before calling the backend", async () => {
    const api = mockApi();
    let resolve!: () => void;
    api.saveDraft.mockReturnValue(new Promise((done) => { resolve = done; }));
    const { entry } = await setupWithFakeTimers(api);
    type(entry, "racing");
    await tick(AUTOSAVE_DELAY_MS);
    expect(api.saveDraft).toHaveBeenCalledOnce();
    key(entry, "Enter");
    await tick();
    expect(api.submit).not.toHaveBeenCalled();
    resolve();
    await tick();
    expect(api.submit).toHaveBeenCalledExactlyOnceWith("racing");
    expect(api.exit).toHaveBeenCalledOnce();

    const other = mockApi();
    let finish!: () => void;
    other.saveDraft.mockReturnValue(new Promise((done) => { finish = done; }));
    cleanups.pop()?.();
    document.body.replaceChildren();
    vi.useRealTimers();
    const view = await setupWithFakeTimers(other);
    type(view.entry, "escaping");
    await tick(AUTOSAVE_DELAY_MS);
    key(view.entry, "Escape");
    await tick();
    expect(other.exit).not.toHaveBeenCalled();
    finish();
    await tick();
    expect(other.exit).toHaveBeenCalledExactlyOnceWith("escaping");
  });

  it("retries quietly when the backend is busy", async () => {
    const api = mockApi();
    api.saveDraft.mockRejectedValueOnce({ code: "busy", message: "An operation is already in progress." });
    const { entry, message } = await setupWithFakeTimers(api);
    type(entry, "retry me");
    await tick(AUTOSAVE_DELAY_MS);
    expect(message.textContent).toBe("");
    await tick(AUTOSAVE_DELAY_MS);
    expect(api.saveDraft).toHaveBeenCalledTimes(2);
    expect(api.saveDraft).toHaveBeenLastCalledWith("retry me");
  });

  it("shows autosave failures and clears them after a later success", async () => {
    const api = mockApi();
    api.saveDraft.mockRejectedValueOnce({ code: "draft_io", message: "Disk full" });
    const { entry, message, notice } = await setupWithFakeTimers(api);
    type(entry, "first");
    await tick(AUTOSAVE_DELAY_MS);
    expect(message.textContent).toBe("Could not autosave the draft: Disk full");
    expect(message.className).toBe("error");
    expect(notice.textContent).toBe("");
    await tick(AUTOSAVE_MAX_WAIT_MS);
    expect(api.saveDraft).toHaveBeenCalledOnce();
    type(entry, "second");
    await tick(AUTOSAVE_DELAY_MS);
    expect(api.saveDraft).toHaveBeenLastCalledWith("second");
    expect(message.textContent).toBe("");
    expect(entry.value).toBe("second");
  });

  it("does not clear unrelated errors after a successful autosave", async () => {
    const api = mockApi();
    api.loadSettings.mockRejectedValue(new Error("Fix configuration"));
    const { entry, message } = await setupWithFakeTimers(api);
    type(entry, "text");
    await tick(AUTOSAVE_DELAY_MS);
    expect(api.saveDraft).toHaveBeenCalledOnce();
    expect(message.textContent).toBe("Fix configuration");
  });

  it("an OS close request exits with the latest text", async () => {
    const api = mockApi();
    const { entry } = await setupWithFakeTimers(api);
    expect(api.onCloseRequested).toHaveBeenCalledOnce();
    type(entry, "unsaved at quit");
    closeRequested!();
    await tick();
    expect(api.exit).toHaveBeenCalledExactlyOnceWith("unsaved at quit");
  });

  it("ignores an OS close request during an operation", async () => {
    const api = mockApi();
    let resolve!: (result: SavedEntry) => void;
    api.submit.mockReturnValue(new Promise((done) => { resolve = done; }));
    const { entry } = await setupWithFakeTimers(api);
    entry.value = "saving";
    key(entry, "Enter");
    await tick();
    closeRequested!();
    await tick();
    expect(api.exit).not.toHaveBeenCalled();
    resolve({ note_path: "today.md" });
    await tick();
    expect(api.exit).toHaveBeenCalledExactlyOnceWith(undefined);
  });

  it("unsubscribes from close requests on unmount", async () => {
    await setup();
    cleanups.pop()?.();
    expect(unlistenClose).toHaveBeenCalledOnce();
  });
});
