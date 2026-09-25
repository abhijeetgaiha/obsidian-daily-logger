# Journal Logger

A frameless journal-entry window built with Tauri 2, framework-free TypeScript,
and a pure Rust logging core. The same source builds on Windows x64 and macOS
Apple Silicon. No Python, Node, or development server is needed at runtime.

## Use

Launch normally and type into the focused text box.

- **Enter** saves and exits; **Shift+Enter** inserts a newline.
- **Escape** keeps the current text as a draft and exits without logging it.
  The draft is restored on the next launch. Closing is blocked during an active operation.
- **Use yesterday if today is missing** is initially unchecked. Changes persist
  immediately. Today always takes priority.
- The bottom-right corner shows the daily note's file name without its path or
  extension, e.g. `2026-09-26`. If today's note is missing, yesterday's name is
  shown even while the checkbox is unchecked, dimmed to show it will not be used
  until the box is ticked. When neither note exists, or the configuration is
  invalid, it shows **No File Selected**. The name refreshes at startup, after
  checkbox changes, and on each Enter.
- Errors retain the draft. Failed preference writes restore the saved checkbox.

There is no tray, global shortcut, background mode, folder picker, or command-line
logging interface. Drag the narrow empty strip above the text box to move it.

Drafts are stored as UTF-8 plaintext in `draft.txt` beside `config.json`, not in
the journal. They work even when vault configuration is missing or invalid.
Escape preserves all text, including whitespace and line breaks; dismissing an
empty text box removes the previous draft. A successful journal save clears the
draft before exiting. If saving or clearing the draft fails, the window stays
open with an error; a journal entry that has already been saved cannot be
submitted again. Fix the storage problem and press Escape to retry closing.

If a draft cannot be read at startup, editing is blocked to avoid overwriting it.
Correct the storage problem and press Enter to reload, or Escape to exit leaving
the file untouched. Only Escape persists the latest edits: operating-system
close/quit and forced termination do not update an unfinished draft.

### Configure the journal folder

Create `config.json` in the application's per-user configuration directory.
Configuration errors display the exact expected path:

| OS | Location |
| --- | --- |
| Windows | `%APPDATA%\local.journal.logger\config.json` |
| macOS | `config.json` inside `local.journal.logger` in your user's Library > Application Support |

Copy `config.example.json` and set `vault_root` to an **absolute path to an
existing journal folder**. On Windows, escape backslashes in JSON:

```json
{
  "vault_root": "C:\\Notes\\Journal",
  "use_yesterday_if_today_missing": false
}
```

On macOS, use the absolute POSIX path to the journal folder. Paths are literal:
`~` and environment-variable placeholders are not expanded. Create the config
directory if needed. The app never chooses a default vault or replaces malformed
configuration. Unknown fields and wrong types are rejected; the fallback field
may be omitted and defaults to `false`.

Configuration reloads before every save and checkbox change. Correct the file
externally and press Enter to retry without losing your draft. Checkbox changes
rewrite the JSON, preserving the configured root but not formatting. Keep personal
configuration and notes outside the source repository.

### Note-writing contract

Daily notes must already exist in the layout
`daily` > `YYYY` > `YYYY-MM` > `YYYY-MM-DD.md`.

The Rust core ports the inspected `log.py` behavior without shipping or running it:

- Capture local time once and prefix the outer-trimmed entry with a timestamp
  such as `[1:05pm]`. Preserve internal spaces, Unicode, and line breaks.
- Use today's note. If missing, unchecked fallback reports an error; checked
  fallback uses yesterday automatically. Never create notes. Yesterday is the
  previous calendar date, including across DST.
- Require exactly one `# Journal` line, optionally followed by spaces or tabs.
  Insert before the first later `## ` heading or at EOF.
- Follow the original text rules, not a Markdown parser: another H1 does not end
  the section, H3 is not a delimiter, and fenced headings are not ignored.
- Preserve UTF-8 BOM and existing content. Detect LF/CRLF from the first LF for
  inserted separators; do not normalize entry-internal newlines. Invalid UTF-8
  and missing/duplicate Journal headings are errors.
- Write and sync a same-directory temporary file, preserve permissions, and
  atomically replace the note. Failed writes never truncate the destination.

The checkbox deliberately replaces the script's confirmation prompt/`--yes`;
explicit configuration replaces its script-directory default.

**Limitations:** atomic replacement prevents partial contents, not conflicts with
simultaneous edits by Obsidian, synchronization software, or another app instance.
Avoid editing the same note during a save, and use one logger window at a time:
there is one shared saved draft per user. Drafts are not continuously autosaved.
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

Windows ARM developers can build the supported x64 package with an x64 Rust
toolchain under Windows emulation and
`npm run tauri build -- --target x86_64-pc-windows-msvc`.
Windows ARM packaging is not a release target.

There is no Python sidecar, shell plugin, or JavaScript filesystem plugin.
Rust handles all journal access and timestamps. Assets are local, with a
restrictive CSP and window-scoped command permissions.

## GitHub Actions and unsigned downloads

`ci.yml` runs frontend and Rust checks and compiles the **full Tauri application**
on Windows x64 and native ARM64 macOS, from the same commit. Both must pass.

`build.yml` runs manually or on `v*` tags and uploads private Windows NSIS and
macOS app/DMG artifacts. No public release or updater is created. Workflows use
pinned actions, locked dependency resolution, and read-only repository
permissions. The account needs an available private-repository Actions allowance.

Artifacts are personal-use builds without trusted Windows signing or Apple
Developer ID signing/notarization. SmartScreen or Gatekeeper may warn or block
opening them. Use the OS's per-app approval flow only after verifying the source
and download; do not disable system-wide protection. Apple Silicon packaging may
use ad-hoc signing, which is not notarization.

Native builds and unit tests do not establish graphical behavior on a Mac.
Manually check focus, keys, sticky preferences, errors, dragging, and note output
against a disposable vault, plus the downloaded app's Gatekeeper experience.
