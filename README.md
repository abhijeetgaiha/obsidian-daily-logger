# Journal Logger

A frameless journal-entry window built with Tauri 2, framework-free TypeScript,
and a pure Rust logging core. The same source builds on Windows x64, Windows
ARM64, and macOS Apple Silicon. No Python, Node, or development server is needed
at runtime.

## Use

Launch normally and type into the focused text box.

- **Enter** saves and exits; **Shift+Enter** inserts a newline.
- **Escape** keeps the current text as a draft and exits without logging it.
  The draft is restored on the next launch. Closing is blocked during an active operation.
- The draft **autosaves** 1 second after you stop typing (and at least every
  10 seconds while typing continuously), and immediately when the window loses
  focus. A brief **Draft saved** appears beside the file name.
- **Use yesterday if today is missing** is initially unchecked. Changes persist
  immediately. Today always takes priority.
- The bottom-right corner shows the daily note's file name without its path or
  extension, e.g. `2026-09-26`. If today's note is missing, yesterday's name is
  shown even while the checkbox is unchecked, dimmed to show it will not be used
  until the box is ticked. When neither note exists, or the configuration is
  invalid, it shows **No File Selected**. The name refreshes at startup, after
  checkbox changes, after saving settings, and on each Enter.
- The **gear** to the right of the file name opens the settings dialog for the
  standard `config.json` (see below). Escape or Cancel closes the dialog without
  exiting; Enter does not log the entry while it is open.
- Errors retain the draft. Failed preference writes restore the saved checkbox.

There is no tray, global shortcut, background mode, or command-line logging
interface. Drag the narrow empty strip above the text box to move it.

Drafts are stored as UTF-8 plaintext in `draft.txt` beside `config.json`, not in
the journal. They work even when vault configuration is missing or invalid.
Autosave and Escape preserve all text, including whitespace and line breaks; dismissing an
empty text box (or autosaving one) removes the previous draft. A successful
journal save clears the
draft before exiting. If saving or clearing the draft fails, the window stays
open with an error; a journal entry that has already been saved cannot be
submitted again. Fix the storage problem and press Escape to retry closing.

If a draft cannot be read at startup, editing is blocked to avoid overwriting it.
Correct the storage problem and press Enter to reload, or Escape to exit leaving
the file untouched; autosave is disabled until the draft loads.

Operating-system close/quit (Cmd+Q, Cmd+W, Alt+F4) saves the latest text and
exits exactly like Escape. If the window does not respond within 3 seconds, the
app exits keeping the last autosaved draft. Autosave failures appear as an error
and are retried on the next edit. Forced termination or a crash can lose up to
the last second of typing (or up to 10 seconds during continuous typing).

### Configure the journal folder

Click the **gear** in the bottom-right corner. The settings dialog always uses
`config.json` in the application's per-user configuration directory; there is no
option to choose a different settings file.

- **No file yet:** the form starts empty. Use **Choose…** to pick the journal
  folder with the system folder picker, then **Save** to create the file (and its
  directory).
- **Valid file:** the current settings are loaded for editing.
- **Insert under heading:** a dropdown with **End of file** (append to the end
  of the note) followed by every distinct heading in the daily note that the
  status row shows (today's, or yesterday's if today's is missing), read from the
  folder selected in the dialog and rescanned after **Choose…**. Without a note
  or folder, only End of file is offered. If the saved heading is not in that
  note, End of file is selected and a notice says so; Cancel keeps the saved
  setting. To use a new heading, add it to the daily note and reopen Settings.
  **If the heading appears more than once** chooses between showing an error
  (default), using the first, or using the last occurrence.
- **Invalid file:** every valid value is filled in and the problems are listed.
  Pick new settings and **Save** to replace the file. Unknown fields are dropped.

Settings are validated before anything is written; save errors stay in the dialog
and never affect your draft. Settings cannot be changed after an entry is logged.

You can also edit the file by hand. Configuration errors display the exact
expected path:

| OS | Location |
| --- | --- |
| Windows | `%APPDATA%\local.journal.logger\config.json` |
| macOS | `config.json` inside `local.journal.logger` in your user's Library > Application Support |

Copy `config.example.json` and set `vault_root` to an **absolute path to an
existing journal folder**. On Windows, escape backslashes in JSON:

```json
{
  "vault_root": "C:\\Notes\\Journal",
  "heading": "# Journal",
  "duplicate_heading": "error",
  "use_yesterday_if_today_missing": false
}
```

On macOS, use the absolute POSIX path to the journal folder. Paths are literal:
`~` and environment-variable placeholders are not expanded. Create the config
directory if needed. The app never chooses a default vault, and it replaces
malformed configuration only when you save from the settings dialog. Unknown
fields and wrong types are rejected. Optional fields: `heading` defaults to `""`
(end of file), `duplicate_heading` to `"error"` (or `"first"`/`"last"`), and the
fallback to `false`. Configurations written before the heading setting existed
therefore append to the end of the note until a heading is set. A hand-edited
`heading` may be any Markdown heading (1–6 `#`, a space, then text), even one
not in today's note.

Configuration reloads before every save and checkbox change. Correct the file
(externally or with the gear) and press Enter to retry without losing your draft.
Checkbox changes and the settings dialog rewrite the JSON without preserving
formatting; the checkbox keeps the configured root and heading. Keep personal
configuration and notes outside the source repository.

### Note-writing contract

Daily notes must already exist in the layout
`daily` > `YYYY` > `YYYY-MM` > `YYYY-MM-DD.md`.

The Rust core is based on the inspected `log.py` behavior, without shipping or
running it, but with a configurable heading:

- Capture local time once and prefix the outer-trimmed entry with a timestamp
  such as `[1:05pm]`. Preserve internal spaces, Unicode, and line breaks.
- Use today's note. If missing, unchecked fallback reports an error; checked
  fallback uses yesterday automatically. Never create notes. Yesterday is the
  previous calendar date, including across DST.
- With no heading configured, append at the end of the note, separated by a
  blank line (an empty note receives just the entry).
- With a heading, find lines exactly equal to it (case-sensitive, same number of
  `#`), optionally followed by spaces or tabs. A missing heading is an error;
  duplicates are an error unless the first or last occurrence is selected.
  Insert before the next heading of **any** level (`#` to `######`, a space, and
  text) or at EOF. `#tag`, a bare `##`, and lines with 7+ `#` are not headings.
- Follow text rules, not a Markdown parser: fenced headings are not ignored.
- Preserve UTF-8 BOM and existing content. Detect LF/CRLF from the first LF for
  inserted separators; do not normalize entry-internal newlines. Invalid UTF-8
  is an error.
- Write and sync a same-directory temporary file, preserve permissions, and
  atomically replace the note. Failed writes never truncate the destination.

The checkbox deliberately replaces the script's confirmation prompt/`--yes`;
explicit configuration replaces its script-directory default.

**Limitations:** atomic replacement prevents partial contents, not conflicts with
simultaneous edits by Obsidian, synchronization software, or another app instance.
Avoid editing the same note during a save, and use one logger window at a time:
there is one shared saved draft per user, and each window autosaves over it.
After an unexpected IPC/worker failure, check the note before
relaunching and retrying.

## Build from source

The Cargo workspace separates `crates\journal-core` (no Tauri dependency),
`src-tauri` (native adapter), and `src` (TypeScript UI). Shared scripts are
platform-neutral; Tauri merges the matching OS-specific bundle configuration.
Both dependency lockfiles are committed.

### Prerequisites

On both platforms, install stable Rust via rustup with `rustfmt` and `clippy`,
and Node.js **24 LTS** with npm.

**Windows x64:** Visual Studio 2022 Build Tools with Desktop development with C++,
MSVC x64 tools, and a Windows SDK; the `x86_64-pc-windows-msvc` Rust toolchain;
WebView2 Evergreen Runtime. The installer downloads WebView2 if absent, requiring
network access for that initial setup.

**Windows ARM64:** as for x64, plus the Visual Studio component "MSVC v143 - VS 2022
C++ ARM64/ARM64EC build tools" and the `aarch64-pc-windows-msvc` Rust target
(`rustup target add aarch64-pc-windows-msvc`). An x64 Rust toolchain running under
emulation can cross-compile it.

**macOS Apple Silicon:** Xcode Command Line Tools (`xcode-select --install`) and
the `aarch64-apple-darwin` Rust toolchain. WebKit is provided by macOS.
The configured minimum macOS version is 11.0. No Windows machine or generated
Windows files are required.

See [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/).
macOS builds run on macOS, not through Windows cross-compilation.

### Develop and verify

From a fresh clone on either supported host:

```text
npm ci
npm run tauri dev
```

Shared automated checks:

```text
npm run check
npm test -- --run
npm run build
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
npm run tauri build -- --no-bundle
```

Core-only tests: `cargo test -p journal-core --locked`. Tests use synthetic
notes and temporary folders, never the real journal.

### Package

```text
npm run tauri build
```

Windows produces an unsigned NSIS installer; macOS produces an Apple Silicon
`.app` and `.dmg`. Outputs are below the workspace `target` directory; explicit
`--target` builds use that target's subdirectory.

Windows ARM64 needs an explicit target, because the packager names the installer
after the host OS rather than the compiled binary. On an ARM64 PC with an x64 Rust
toolchain, a plain build produces an x64 app in an installer labelled `arm64`:

```text
npm run tauri build -- --target aarch64-pc-windows-msvc
npm run tauri build -- --target x86_64-pc-windows-msvc
```

There is no Python sidecar, shell plugin, or JavaScript filesystem plugin.
Rust handles all journal access and timestamps. The folder picker comes from
`tauri-plugin-dialog`, invoked only from Rust; the webview is granted no dialog
or filesystem permissions. Assets are local, with a
restrictive CSP and window-scoped command permissions.

## GitHub Actions and unsigned downloads

`ci.yml` runs frontend and Rust checks and compiles the **full Tauri application**
on Windows x64, native Windows ARM64, and native ARM64 macOS, from the same commit.
All must pass.

`build.yml` runs manually or on `v*` tags and uploads private Windows x64 and ARM64
NSIS installers plus macOS app/DMG artifacts. No public release or updater is created. Workflows use
pinned actions, locked dependency resolution, and read-only repository
permissions. The account needs an available private-repository Actions allowance.

Artifacts are personal-use builds without trusted Windows signing or Apple
Developer ID signing/notarization. SmartScreen or Gatekeeper may warn or block
opening them. Use the OS's per-app approval flow only after verifying the source
and download; do not disable system-wide protection. The macOS bundle is ad-hoc
signed (`signingIdentity: "-"`), which is not notarization: after the first
blocked launch, approve it in System Settings > Privacy & Security > Open Anyway.
If macOS says the app "is damaged", the bundle signature is missing or broken;
`codesign --verify --deep --strict` on the `.app` must pass.

Native builds and unit tests do not establish graphical behavior on a Mac.
Manually check focus, keys, sticky preferences, the settings dialog and folder
picker, errors, dragging, and note output
against a disposable vault, plus the downloaded app's Gatekeeper experience.
