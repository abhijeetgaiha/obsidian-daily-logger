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
  valid?(form: SettingsForm): boolean;
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

// Mirrors journal_core::validate_heading; the backend validates authoritatively on save.
export function headingProblem(heading: string): string | undefined {
  const trimmed = heading.trim();
  if (trimmed === "" || /^#{1,6}[ \t]+\S/.test(trimmed)) return undefined;
  return "Use 1–6 # characters, a space, and text, e.g. # Journal or ## Daily Log.";
}

function headingField(_api: JournalApi, context: FieldContext): Field {
  const element = document.createElement("div");
  element.className = "setting";
  element.innerHTML = `
    <label class="setting-label" for="settings-heading">Insert under heading</label>
    <input id="settings-heading" type="text" placeholder="Empty: add to end of file"
      spellcheck="false" autocomplete="off" aria-describedby="settings-heading-problem" />
    <p id="settings-heading-problem" class="error"></p>
    <label class="sub-setting">
      If the heading appears more than once
      <select id="settings-duplicates">
        <option value="error">Show an error</option>
        <option value="first">Use the first</option>
        <option value="last">Use the last</option>
      </select>
    </label>
  `;
  const input = element.querySelector<HTMLInputElement>("#settings-heading")!;
  const problem = element.querySelector<HTMLParagraphElement>("#settings-heading-problem")!;
  const select = element.querySelector<HTMLSelectElement>("#settings-duplicates")!;
  const onInput = () => context.update({ heading: input.value });
  const onChange = () => context.update({
    duplicate_heading: select.value as DuplicateHeading,
  });
  input.addEventListener("input", onInput);
  select.addEventListener("change", onChange);
  return {
    element,
    render(form, disabled) {
      if (input.value !== form.heading) input.value = form.heading;
      input.disabled = disabled;
      const message = headingProblem(form.heading);
      problem.textContent = message ?? "";
      input.setAttribute("aria-invalid", String(message !== undefined));
      select.value = form.duplicate_heading;
      select.disabled = disabled || form.heading.trim() === "";
    },
    valid: (form) => headingProblem(form.heading) === undefined,
    destroy() {
      input.removeEventListener("input", onInput);
      select.removeEventListener("change", onChange);
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

  function render() {
    overlay.hidden = !open;
    fields.forEach((field) => field.render(form, busy));
    cancelButton.disabled = busy;
    saveButton.disabled = busy || form.vault_root === null
      || fields.some((field) => field.valid?.(form) === false);
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
      render();
      overlay.querySelector<HTMLButtonElement>("#settings-choose")!.focus();
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
