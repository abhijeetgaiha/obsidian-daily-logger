import {
  errorMessage, type DuplicateHeading, type FormResult, type JournalApi, type Settings,
  type SettingsForm,
} from "./api";

interface FieldContext {
  form(): SettingsForm;
  update(change: Partial<SettingsForm>): void;
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
    <span class="setting-label" id="settings-folder-label">Journal folder</span>
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

function headingField(api: JournalApi, context: FieldContext): Field {
  const element = document.createElement("div");
  element.className = "setting";
  element.innerHTML = `
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
  `;
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
      const { vault_root: root } = context.form();
      let list;
      try {
        list = await api.listHeadings(root);
      } catch (error) {
        setNote(`Could not list headings: ${errorMessage(error)}`, "error");
        return;
      }
      setOptions(list.headings);
      if (list.problem !== null) {
        setNote(list.problem, "error");
        context.update({});
        return;
      }
      if (list.note === null) {
        setNote(root === null
          ? "Choose a journal folder to list its headings."
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
      headingSelect.removeEventListener("change", onHeadingChange);
      duplicates.removeEventListener("change", onDuplicatesChange);
    },
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

const fieldFactories = [folderField, headingField, fallbackField];

export interface SettingsHooks {
  onSaved(settings: Settings): void;
  onClose(): void;
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
      <h2 id="settings-title">Settings</h2>
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
  const fieldsHost = overlay.querySelector<HTMLDivElement>("#settings-fields")!;
  const message = overlay.querySelector<HTMLParagraphElement>("#settings-message")!;
  const cancelButton = overlay.querySelector<HTMLButtonElement>("#settings-cancel")!;
  const saveButton = overlay.querySelector<HTMLButtonElement>("#settings-save")!;
  let form: SettingsForm = {
    vault_root: null,
    heading: "",
    duplicate_heading: "error",
    use_yesterday_if_today_missing: false,
  };
  let open = false;
  let busy = false;
  let refreshNeeded = false;

  function render() {
    overlay.hidden = !open;
    fields.forEach((field) => field.render(form, busy));
    cancelButton.disabled = busy;
    saveButton.disabled = busy || form.vault_root === null;
    overlay.setAttribute("aria-busy", String(busy));
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
    update(change) {
      if (change.vault_root !== undefined && change.vault_root !== form.vault_root) {
        refreshNeeded = true;
      }
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
