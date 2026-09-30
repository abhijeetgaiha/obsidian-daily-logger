import {
  acceptCompletion, autocompletion, completionStatus, startCompletion, type Completion,
  type CompletionContext, type CompletionResult,
} from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, insertNewline } from "@codemirror/commands";
import { defineLanguageFacet, Language, syntaxTree } from "@codemirror/language";
import { Compartment, EditorState, Prec, type Range, Transaction } from "@codemirror/state";
import {
  Decoration, type DecorationSet, EditorView, keymap, placeholder, ViewPlugin, type ViewUpdate,
} from "@codemirror/view";
import { parser as commonmark } from "@lezer/markdown";
import type { NoteLink } from "./api";

// Plain CommonMark: only its emphasis and list nodes are used, for styling.
const markdownLanguage = new Language(defineLanguageFacet(), commonmark, [], "markdown");

/** Suggestions offered after `[[`; more matches are left out. */
export const MAX_SUGGESTIONS = 100;

export interface EditorOptions {
  placeholder: string;
  label: string;
  onChange(): void;
  /** Notes offered after `[[`. */
  notes(): readonly NoteLink[];
  /** Called when a `[[` suggestion list is requested, e.g. to refresh a stale index. */
  onLinkQuery?(): void;
}

export interface JournalEditor {
  readonly view: EditorView;
  value(): string;
  setValue(text: string): void;
  setReadOnly(readOnly: boolean): void;
  focus(): void;
  moveCursorToEnd(): void;
  destroy(): void;
}

// The typed text between an unclosed `[[` and the cursor.
const LINK_QUERY = /\[\[[^[\]|#\n]*$/;
const WIKILINK = /\[\[[^[\]\n]+\]\]/g;

function score(note: NoteLink, query: string): number | undefined {
  const name = note.name.toLowerCase();
  if (!query) return 0;
  if (name.startsWith(query)) return 0;
  const index = name.indexOf(query);
  if (index > 0 && /[\s\-_.,()]/.test(name[index - 1]!)) return 1;
  if (index > 0) return 2;
  const path = note.folder ? `${note.folder}/${note.name}`.toLowerCase() : name;
  return path.includes(query) ? 3 : undefined;
}

/** Notes matching `query` (case-insensitive): name prefixes, then word starts, then any part. */
export function rankNotes(notes: readonly NoteLink[], query: string): NoteLink[] {
  const needle = query.trim().toLowerCase();
  const scored: { note: NoteLink; score: number }[] = [];
  for (const note of notes) {
    const value = score(note, needle);
    if (value !== undefined) scored.push({ note, score: value });
  }
  scored.sort((a, b) =>
    a.score - b.score ||
    a.note.name.length - b.note.name.length ||
    a.note.name.localeCompare(b.note.name) ||
    a.note.folder.localeCompare(b.note.folder));
  return scored.slice(0, MAX_SUGGESTIONS).map(({ note }) => note);
}

function linkCompletion(note: NoteLink): Completion {
  return {
    label: note.name,
    detail: note.folder,
    apply(view, _completion, from, to) {
      const closed = view.state.sliceDoc(to, to + 2) === "]]";
      const insert = closed ? note.link : `${note.link}]]`;
      view.dispatch({
        changes: { from, to, insert },
        selection: { anchor: from + note.link.length + 2 },
        userEvent: "input.complete",
      });
    },
  };
}

function wikiLinkSource(options: EditorOptions) {
  return (context: CompletionContext): CompletionResult | null => {
    const match = context.matchBefore(LINK_QUERY);
    if (!match) return null;
    options.onLinkQuery?.();
    const notes = rankNotes(options.notes(), match.text.slice(2));
    if (!notes.length) return null;
    return { from: match.from + 2, options: notes.map(linkCompletion), filter: false };
  };
}

// Approximate widths in half `ch` units, for the hanging indent of wrapped list items.
function halfWidths(text: string): number {
  let width = 0;
  for (const char of text) {
    if (char === "\t") width += 4;
    else if (char === " " || char === "." || char === ")") width += 1;
    else width += 2;
  }
  return width;
}

const MAX_HANG = 40;

function formatting(view: EditorView): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  const mark = (className: string) => Decoration.mark({ class: className });
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(view.state).iterate({
      from, to,
      enter(node) {
        switch (node.name) {
          case "StrongEmphasis":
            ranges.push(mark("cm-md-strong").range(node.from, node.to));
            break;
          case "Emphasis":
            ranges.push(mark("cm-md-em").range(node.from, node.to));
            break;
          case "EmphasisMark":
            ranges.push(mark("cm-md-mark").range(node.from, node.to));
            break;
          case "ListMark": {
            const list = node.node.parent?.parent?.name;
            if (list !== "BulletList" && list !== "OrderedList") break;
            const line = view.state.doc.lineAt(node.from);
            const prefix = view.state.sliceDoc(line.from, Math.min(node.to + 1, line.to));
            const hang = Math.min(MAX_HANG, halfWidths(prefix));
            const kind = list === "BulletList" ? "cm-md-bullet" : "cm-md-number";
            ranges.push(Decoration.line({ class: `cm-md-list cm-md-hang-${hang}` }).range(line.from));
            ranges.push(mark(`cm-md-list-mark ${kind}`).range(node.from, node.to));
            break;
          }
        }
      },
    });
    const text = view.state.sliceDoc(from, to);
    for (const match of text.matchAll(WIKILINK)) {
      const start = from + match.index;
      const end = start + match[0].length;
      ranges.push(mark("cm-md-wikilink").range(start, end));
      ranges.push(mark("cm-md-mark").range(start, start + 2));
      ranges.push(mark("cm-md-mark").range(end - 2, end));
    }
  }
  return Decoration.set(ranges, true);
}

const markdownFormatting = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = formatting(view);
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged ||
        syntaxTree(update.startState) !== syntaxTree(update.state)) {
        this.decorations = formatting(update.view);
      }
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

const hangRules = Object.fromEntries(
  Array.from({ length: MAX_HANG }, (_, index) => {
    const half = index + 1;
    return [`.cm-md-hang-${half}`, {
      paddingLeft: `calc(7px + ${half / 2}ch)`,
      textIndent: `-${half / 2}ch`,
    }];
  }),
);

// Colours come from CSS variables in styles.css, which inherit into the shadow root.
const journalTheme = EditorView.theme({
  "&": { height: "100%", color: "inherit", backgroundColor: "transparent", borderRadius: "3px" },
  "&.cm-focused": { outline: "1px solid #70899e" },
  ".cm-scroller": { fontFamily: "inherit", lineHeight: "1.5", overflowX: "hidden" },
  ".cm-content": { padding: "7px 0", caretColor: "auto" },
  ".cm-line": { padding: "0 7px" },
  ".cm-placeholder": { color: "var(--placeholder)" },
  ".cm-md-strong": { fontWeight: "700" },
  ".cm-md-em": { fontStyle: "italic" },
  ".cm-md-mark": { color: "var(--md-mark)" },
  ".cm-md-list-mark": { color: "var(--md-list)" },
  ".cm-md-number": { fontVariantNumeric: "tabular-nums" },
  ".cm-md-wikilink": { color: "var(--md-link)" },
  ...hangRules,
  ".cm-tooltip": {
    color: "inherit",
    backgroundColor: "var(--popup-bg)",
    border: "1px solid var(--popup-border)",
    borderRadius: "4px",
    boxShadow: "0 2px 8px rgb(0 0 0 / 0.18)",
    overflow: "hidden",
  },
  ".cm-tooltip.cm-tooltip-autocomplete > ul": {
    fontFamily: "inherit",
    fontSize: "13px",
    maxHeight: "11em",
    minWidth: "220px",
    maxWidth: "min(460px, calc(100vw - 24px))",
  },
  ".cm-tooltip.cm-tooltip-autocomplete > ul > li": {
    padding: "2px 8px",
    overflow: "hidden",
    textOverflow: "ellipsis",
  },
  ".cm-tooltip-autocomplete ul li[aria-selected]": {
    color: "inherit",
    backgroundColor: "var(--popup-selected)",
  },
  ".cm-completionLabel": { fontWeight: "500" },
  ".cm-completionDetail": { marginLeft: "10px", fontStyle: "normal", opacity: "0.6" },
  ".cm-completionMatchedText": { textDecoration: "none" },
});

/**
 * Mounts a Markdown editor in an open shadow root of `host`. Inside a shadow root CodeMirror
 * uses constructable stylesheets, which the app's strict `style-src` CSP allows.
 */
export function createEditor(host: HTMLElement, options: EditorOptions): JournalEditor {
  const shadow = host.shadowRoot ?? host.attachShadow({ mode: "open" });
  const readOnly = new Compartment();
  const readOnlyState = (value: boolean) =>
    [EditorState.readOnly.of(value), EditorView.editable.of(!value)];
  const view = new EditorView({
    root: shadow,
    parent: shadow,
    state: EditorState.create({
      extensions: [
        history(),
        markdownLanguage.extension,
        markdownFormatting,
        EditorView.lineWrapping,
        placeholder(options.placeholder),
        EditorView.contentAttributes.of({
          "aria-label": options.label,
          spellcheck: "true",
          autocorrect: "on",
          autocapitalize: "sentences",
        }),
        autocompletion({
          override: [wikiLinkSource(options)],
          icons: false,
          maxRenderedOptions: MAX_SUGGESTIONS,
        }),
        // Tab accepts a suggestion; Enter and Escape reach the window's handler when no list is open.
        Prec.highest(keymap.of([{ key: "Tab", run: acceptCompletion }])),
        keymap.of([
          { key: "Shift-Enter", run: insertNewline },
          // Enter with any modifier but Shift saves, and Escape exits, in the window's handler.
          ...defaultKeymap.filter(({ key }) => !key?.endsWith("Enter") && key !== "Escape"),
          ...historyKeymap,
        ]),
        readOnly.of(readOnlyState(true)),
        journalTheme,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) options.onChange();
        }),
      ],
    }),
  });
  return {
    view,
    value: () => view.state.doc.toString(),
    setValue(text) {
      // Not undoable: undo must never erase a restored draft.
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: text },
        annotations: Transaction.addToHistory.of(false),
      });
    },
    setReadOnly(value) {
      if (view.state.readOnly === value) return;
      view.dispatch({ effects: readOnly.reconfigure(readOnlyState(value)) });
    },
    focus: () => view.focus(),
    moveCursorToEnd() {
      view.dispatch({ selection: { anchor: view.state.doc.length } });
    },
    destroy: () => view.destroy(),
  };
}

/** Whether a `[[` suggestion list is open. */
export function hasOpenCompletion(editor: JournalEditor): boolean {
  return completionStatus(editor.view.state) === "active";
}

/** Re-queries suggestions if the cursor is in an unclosed `[[`, e.g. after the index loads. */
export function refreshLinkCompletion(editor: JournalEditor): void {
  const { state } = editor.view;
  const { head } = state.selection.main;
  const line = state.doc.lineAt(head);
  if (editor.view.hasFocus && LINK_QUERY.test(state.sliceDoc(line.from, head))) {
    startCompletion(editor.view);
  }
}
