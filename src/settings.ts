import {
  errorMessage, type FormResult, type JournalApi, type Settings, type SettingsForm,
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

const fieldFactories = [folderField, fallbackField];

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
  let form: SettingsForm = { vault_root: null, use_yesterday_if_today_missing: false };
  let open = false;
  let busy = false;

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
