export const AUTOSAVE_DELAY_MS = 1000;
export const AUTOSAVE_MAX_WAIT_MS = 10_000;

/** "ready": save now; "blocked": try again shortly; "disabled": never save. */
export type AutosaveState = "ready" | "blocked" | "disabled";

export interface AutosaveOptions {
  read(): string;
  state(): AutosaveState;
  save(text: string): Promise<void>;
  onSaved(): void;
  onError(error: unknown): void;
}

export interface Autosave {
  /** Call after every edit; saves after a pause, or after the maximum wait. */
  schedule(): void;
  /** Saves pending text now. */
  flush(): Promise<void>;
  /** Waits for an in-flight save so other operations don't contend for the backend. */
  idle(): Promise<void>;
  /** Records text known to be stored, e.g. the restored draft. */
  reset(text: string): void;
  dispose(): void;
}

function isBusy(error: unknown) {
  return typeof error === "object" && error !== null && "code" in error && error.code === "busy";
}

export function createAutosave(options: AutosaveOptions): Autosave {
  let stored: string | undefined;
  let debounce: ReturnType<typeof setTimeout> | undefined;
  let maxWait: ReturnType<typeof setTimeout> | undefined;
  let inFlight: Promise<void> | undefined;
  let disposed = false;

  function clearTimers() {
    clearTimeout(debounce);
    clearTimeout(maxWait);
    debounce = maxWait = undefined;
  }

  function schedule() {
    if (disposed) return;
    if (options.read() === stored) {
      clearTimers();
      return;
    }
    clearTimeout(debounce);
    debounce = setTimeout(() => void flush(), AUTOSAVE_DELAY_MS);
    maxWait ??= setTimeout(() => void flush(), AUTOSAVE_MAX_WAIT_MS);
  }

  async function flush() {
    clearTimers();
    await inFlight;
    if (disposed || inFlight) return;
    const text = options.read();
    if (text === stored) return;
    const state = options.state();
    if (state === "disabled") return;
    if (state === "blocked") {
      schedule();
      return;
    }
    inFlight = (async () => {
      try {
        await options.save(text);
        stored = text;
        options.onSaved();
      } catch (error) {
        if (isBusy(error)) schedule();
        else options.onError(error);
      }
    })();
    try {
      await inFlight;
    } finally {
      inFlight = undefined;
    }
  }

  return {
    schedule,
    flush,
    idle: async () => {
      await inFlight;
    },
    reset(text) {
      stored = text;
      clearTimers();
    },
    dispose() {
      disposed = true;
      clearTimers();
    },
  };
}
