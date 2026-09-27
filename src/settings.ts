import {
  errorMessage, type DuplicateHeading, type EntryFormat, type FormResult, type JournalApi,
  type NoteSource, type Settings, type SettingsForm,
} from "./api";

interface FieldContext {
  form(): SettingsForm;
  /** Changing the folder or note source refreshes dependent data unless `refresh` is false. */
  update(change: Partial<SettingsForm>, options?: { refresh?: boolean }): void;
  run(task: () => Promise<void>): Promise<void>;
}

// Each setting is one field; add new settings by adding a field factory to `fieldFactories`.
interface Field {
  element: HTMLElement;
  render(form: SettingsForm, disabled: boolean): void;
  /** Reloads data that depends on the form; runs on open and when the folder changes. */
  refresh?(): Promise<void>;
  destroy(): void;
}

function folderField(api: JournalApi, context: FieldContext): Field {
  const element = document.createElement("div");
  element.className = "setting";
  element.innerHTML = `
    <span class="setting-label" id="settings-folder-label">Obsidian vault</span>
    <div class="folder">
      <span id="settings-folder" aria-labelledby="settings-folder-label"></span>
      <button id="settings-choose" type="button">Choose…</button>
    </div>
  `;
  const path = element.querySelector<HTMLSpanElement>("#settings-folder")!;
  const choose = element.querySelector<HTMLButtonElement>("#settings-choose")!;
  const onClick = () => context.run(async () => {
    const picked = await api.pickVaultFolder(context.form().vault_root);
    if (picked !== null) context.update({ vault_root: picked });
  });
  choose.addEventListener("click", onClick);
  return {
    element,
    render(form, disabled) {
      path.textContent = form.vault_root ?? "Not set";
      path.title = form.vault_root ?? "";
      path.classList.toggle("unset", form.vault_root === null);
      choose.disabled = disabled;
    },
    destroy: () => choose.removeEventListener("click", onClick),
  };
}

// The note source and heading share one lookup, because headings come from the note it finds.
function noteField(api: JournalApi, context: FieldContext): Field {
  const element = document.createElement("div");
  element.className = "setting-group";
  element.innerHTML = `
    <div class="setting">
      <label class="setting-label" for="settings-source">Daily notes from</label>
      <select id="settings-source" aria-describedby="settings-source-note">
        <option value="" disabled>Choose a plugin</option>
        <option value="periodic">Periodic Notes</option>
        <option value="daily">Daily notes (core plugin)</option>
      </select>
      <p id="settings-source-note"></p>
    </div>
    <div class="setting">
      <label class="setting-label" for="settings-heading">Insert under heading</label>
      <select id="settings-heading" aria-describedby="settings-heading-note"></select>
      <p id="settings-heading-note"></p>
      <label class="sub-setting">
        If the heading appears more than once
        <select id="settings-duplicates">
          <option value="error">Show an error</option>
          <option value="first">Use the first</option>
          <option value="last">Use the last</option>
        </select>
      </label>
    </div>
  `;
  const sourceSelect = element.querySelector<HTMLSelectElement>("#settings-source")!;
  const sourceNote = element.querySelector<HTMLParagraphElement>("#settings-source-note")!;
  const headingSelect = element.querySelector<HTMLSelectElement>("#settings-heading")!;
  const note = element.querySelector<HTMLParagraphElement>("#settings-heading-note")!;
  const duplicates = element.querySelector<HTMLSelectElement>("#settings-duplicates")!;

  function option(value: string, label: string) {
    const item = document.createElement("option");
    item.value = value;
    item.textContent = label;
    return item;
  }

  function setOptions(headings: string[]) {
    headingSelect.replaceChildren(
      option("", "End of file"), ...headings.map((heading) => option(heading, heading)),
    );
  }

  function setNote(text: string, kind: "" | "notice" | "error" = "") {
    note.textContent = text;
    note.className = kind;
  }

  function setSourceNote(text: string, kind: "" | "notice" | "error" = "") {
    sourceNote.textContent = text;
    sourceNote.className = kind;
  }

  const onSourceChange = () => {
    // Read before run() re-renders the select from the form.
    const note_source = sourceSelect.value as NoteSource;
    void context.run(async () => context.update({ note_source }));
  };
  sourceSelect.addEventListener("change", onSourceChange);

  const onHeadingChange = () => context.update({ heading: headingSelect.value });
  const onDuplicatesChange = () => context.update({
    duplicate_heading: duplicates.value as DuplicateHeading,
  });
  headingSelect.addEventListener("change", onHeadingChange);
  duplicates.addEventListener("change", onDuplicatesChange);
  setOptions([]);
  return {
    element,
    render(form, disabled) {
      sourceSelect.value = form.note_source ?? "";
      sourceSelect.disabled = disabled;
      // Keep an unverified saved heading selectable until the note has been scanned.
      if (![...headingSelect.options].some((item) => item.value === form.heading)) {
        headingSelect.append(option(form.heading, form.heading));
      }
      headingSelect.value = form.heading;
      headingSelect.disabled = disabled;
      duplicates.value = form.duplicate_heading;
      duplicates.disabled = disabled || form.heading === "";
    },
    async refresh() {
      const { vault_root: root, note_source: source } = context.form();
      let list;
      try {
        list = await api.listHeadings(root, source);
      } catch (error) {
        setSourceNote("");
        setNote(`Could not list headings: ${errorMessage(error)}`, "error");
        return;
      }
      setOptions(list.headings);
      if (source === null && list.detected !== null) {
        // The lookup already used the detected plugin, so no second refresh is needed.
        context.update({ note_source: list.detected }, { refresh: false });
        setSourceNote(`Detected ${list.layout ?? list.detected}. Save to use it.`, "notice");
      } else {
        setSourceNote(list.layout ?? "");
      }
      if (list.layout === null && list.problem !== null) {
        setSourceNote(list.problem, "error");
        setNote("");
        context.update({});
        return;
      }
      if (list.problem !== null) {
        setNote(list.problem, "error");
        context.update({});
        return;
      }
      if (list.note === null) {
        setNote(root === null
          ? "Choose a vault folder to list its headings."
          : "No daily note for today or yesterday; only End of file is available.");
      } else {
        setNote(`Headings from ${list.note}.`);
      }
      const current = context.form().heading;
      if (current !== "" && !list.headings.includes(current)) {
        context.update({ heading: "" });
        setNote(
          `"${current}" isn't in ${list.note ?? "a daily note"}; End of file selected.`,
          "notice",
        );
      } else {
        context.update({});
      }
    },
    destroy() {
      sourceSelect.removeEventListener("change", onSourceChange);
      headingSelect.removeEventListener("change", onHeadingChange);
      duplicates.removeEventListener("change", onDuplicatesChange);
    },
  };
}

function entryFormatField(_api: JournalApi, context: FieldContext): Field {
  const element = document.createElement("div");
  element.className = "setting";
  element.innerHTML = `
    <label class="setting-label" for="settings-entry-format">Entry format</label>
    <select id="settings-entry-format">
      <option value="inline">Inline: [1:05pm] text</option>
      <option value="block">Block: bold time, text, then ---</option>
    </select>
  `;
  const select = element.querySelector<HTMLSelectElement>("#settings-entry-format")!;
  const onChange = () => context.update({ entry_format: select.value as EntryFormat });
  select.addEventListener("change", onChange);
  return {
    element,
    render(form, disabled) {
      select.value = form.entry_format;
      select.disabled = disabled;
    },
    destroy: () => select.removeEventListener("change", onChange),
  };
}

function fallbackField(_api: JournalApi, context: FieldContext): Field {
  const element = document.createElement("label");
  element.className = "setting fallback";
  element.innerHTML = `
    <input id="settings-fallback" type="checkbox" />
    Use yesterday if today is missing
  `;
  const input = element.querySelector<HTMLInputElement>("#settings-fallback")!;
  const onChange = () => context.update({ use_yesterday_if_today_missing: input.checked });
  input.addEventListener("change", onChange);
  return {
    element,
    render(form, disabled) {
      input.checked = form.use_yesterday_if_today_missing;
      input.disabled = disabled;
    },
    destroy: () => input.removeEventListener("change", onChange),
  };
}

const fieldFactories = [folderField, noteField, entryFormatField, fallbackField];

export interface SettingsHooks {
  onSaved(settings: Settings): void;
  onClose(): void;
  /** Reports the window height, in CSS pixels, that shows the whole dialog while it is open. */
  onResize?(height: number): void;
}

export interface SettingsDialog {
  readonly isOpen: boolean;
  show(result: FormResult): void;
  cancel(): void;
  destroy(): void;
}

export function mountSettingsDialog(
  host: HTMLElement, api: JournalApi, hooks: SettingsHooks,
): SettingsDialog {
  const overlay = document.createElement("div");
  overlay.id = "settings-dialog";
  overlay.className = "overlay";
  overlay.hidden = true;
  overlay.innerHTML = `
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title"
      aria-describedby="settings-issue">
      <h2 id="settings-title" data-tauri-drag-region>Settings</h2>
      <p id="settings-issue"></p>
      <div id="settings-fields"></div>
      <p id="settings-message" role="status" aria-live="polite"></p>
      <div class="actions">
        <button id="settings-cancel" type="button">Cancel</button>
        <button id="settings-save" type="button">Save</button>
      </div>
    </div>
  `;
  host.append(overlay);
  const issue = overlay.querySelector<HTMLParagraphElement>("#settings-issue")!;
  const dialogBox = overlay.querySelector<HTMLDivElement>(".dialog")!;
  const fieldsHost = overlay.querySelector<HTMLDivElement>("#settings-fields")!;
  const message = overlay.querySelector<HTMLParagraphElement>("#settings-message")!;
  const cancelButton = overlay.querySelector<HTMLButtonElement>("#settings-cancel")!;
  const saveButton = overlay.querySelector<HTMLButtonElement>("#settings-save")!;
  let form: SettingsForm = {
    vault_root: null,
    note_source: null,
    heading: "",
    duplicate_heading: "error",
    entry_format: "inline",
    use_yesterday_if_today_missing: false,
  };
  let open = false;
  let busy = false;
  let refreshNeeded = false;

  function render() {
    overlay.hidden = !open;
    fields.forEach((field) => field.render(form, busy));
    cancelButton.disabled = busy;
    saveButton.disabled = busy || form.vault_root === null || form.note_source === null;
    overlay.setAttribute("aria-busy", String(busy));
    // The overlay's 1px top and bottom borders frame the window around the dialog.
    if (open) hooks.onResize?.(dialogBox.scrollHeight + 2);
  }

  async function run(task: () => Promise<void>) {
    if (busy || !open) return;
    busy = true;
    message.textContent = "";
    message.className = "";
    render();
    try {
      await task();
      if (refreshNeeded) {
        refreshNeeded = false;
        await Promise.all(fields.map((field) => field.refresh?.()));
      }
    } catch (error) {
      message.textContent = errorMessage(error);
      message.className = "error";
    } finally {
      busy = false;
      render();
    }
  }

  const context: FieldContext = {
    form: () => form,
    update(change, options) {
      const changed = (["vault_root", "note_source"] as const).some(
        (key) => change[key] !== undefined && change[key] !== form[key],
      );
      if (changed && options?.refresh !== false) refreshNeeded = true;
      form = { ...form, ...change };
      render();
    },
    run,
  };
  const fields = fieldFactories.map((factory) => factory(api, context));
  fieldsHost.append(...fields.map((field) => field.element));

  function close() {
    open = false;
    render();
    hooks.onClose();
  }

  const save = () => run(async () => {
    message.textContent = "Saving...";
    hooks.onSaved(await api.saveSettings({ ...form }));
    close();
  });

  function cancel() {
    if (open && !busy) close();
  }

  saveButton.addEventListener("click", save);
  cancelButton.addEventListener("click", cancel);
  render();

  return {
    get isOpen() {
      return open;
    },
    show(result) {
      form = { ...result.form };
      if (result.issue !== null) {
        issue.textContent = result.issue;
        issue.className = "error";
      } else if (!result.exists) {
        issue.textContent =
          `No settings file exists yet. Saving creates ${result.config_path}.`;
        issue.className = "";
      } else {
        issue.textContent = "";
        issue.className = "";
      }
      message.textContent = "";
      message.className = "";
      open = true;
      refreshNeeded = true;
      render();
      overlay.querySelector<HTMLButtonElement>("#settings-choose")!.focus();
      void run(async () => {});
    },
    cancel,
    destroy() {
      saveButton.removeEventListener("click", save);
      cancelButton.removeEventListener("click", cancel);
      fields.forEach((field) => field.destroy());
      overlay.remove();
    },
  };
}
