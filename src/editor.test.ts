import { afterEach, describe, expect, it, vi } from "vitest";
import { undo } from "@codemirror/commands";
import { EditorView } from "@codemirror/view";
import type { NoteLink } from "./api";
import {
  continueList, createEditor, MAX_SUGGESTIONS, rankNotes, type JournalEditor,
} from "./editor";

const note = (name: string, folder = ""): NoteLink => ({ name, folder, link: name });

describe("rankNotes", () => {
  const notes = [
    note("Weekly plan", "Work"),
    note("Explanation"),
    note("Plan", "B"),
    note("Plan", "A"),
    note("Planning retreat"),
    note("Home"),
    note("Gardening", "Plants"),
  ];
  const names = (query: string) =>
    rankNotes(notes, query).map(({ name, folder }) => (folder ? `${folder}/${name}` : name));

  it("puts name prefixes first, then word starts, then any part of the name, then folders", () => {
    expect(names("plan")).toEqual([
      "A/Plan", "B/Plan", "Planning retreat", "Work/Weekly plan", "Explanation", "Plants/Gardening",
    ]);
  });

  it("is case-insensitive and ignores surrounding spaces", () => {
    expect(names("  HOM ")).toEqual(["Home"]);
    expect(names("zzz")).toEqual([]);
  });

  it("lists every note, shortest names first, for an empty query and caps the list", () => {
    expect(names("")[0]).toBe("Home");
    const many = Array.from({ length: MAX_SUGGESTIONS + 5 }, (_, index) => note(`Note ${index}`));
    expect(rankNotes(many, "note")).toHaveLength(MAX_SUGGESTIONS);
  });
});

describe("Markdown formatting", () => {
  let editor: JournalEditor | undefined;
  afterEach(() => {
    editor?.destroy();
    document.body.innerHTML = "";
  });

  function render(text: string) {
    const host = document.createElement("div");
    document.body.append(host);
    editor = createEditor(host, {
      placeholder: "", label: "Journal entry", onChange() {}, notes: () => [],
    });
    editor.setValue(text);
    const shadow = host.shadowRoot!;
    return {
      editor,
      texts: (selector: string) =>
        [...shadow.querySelectorAll(selector)].map((element) => element.textContent),
      lines: () => [...shadow.querySelectorAll<HTMLElement>(".cm-line")],
      rule: (selector: string) => editor!.view.state.facet(EditorView.styleModule)
        .flatMap((module) => module.getRules().split("\n"))
        .find((rule) => rule.includes(` ${selector} {`)),
    };
  }

  it("styles bold, italic, and wikilinks while keeping the markers visible", () => {
    const view = render("A **bold** and *italic* and __strong__ _em_ [[My Note]] text");
    expect(view.texts(".cm-md-strong")).toEqual(["**bold**", "__strong__"]);
    expect(view.texts(".cm-md-em")).toEqual(["*italic*", "_em_"]);
    expect(view.texts(".cm-md-wikilink")).toEqual(["[[My Note]]"]);
    expect(view.texts(".cm-md-mark")).toEqual(
      ["**", "**", "*", "*", "__", "__", "_", "_", "[[", "]]"],
    );
    expect(view.lines()[0]!.textContent).toBe(
      "A **bold** and *italic* and __strong__ _em_ [[My Note]] text",
    );
  });

  it("styles bullet and numbered list markers with a hanging indent", () => {
    const text = "- bullet\n* star\n+ plus\n1. first\n10) tenth\n  - nested\nplain";
    const view = render(text);
    expect(view.texts(".cm-md-bullet")).toEqual(["\u2022", "*", "+", "\u2022"]);
    expect(view.texts(".cm-md-number")).toEqual(["1.", "10)"]);
    const hangs = view.lines().map((line) =>
      [...line.classList].find((name) => name.startsWith("cm-md-hang-")) ?? null);
    expect(hangs).toEqual([
      "cm-md-hang-3", "cm-md-hang-3", "cm-md-hang-3", "cm-md-hang-4", "cm-md-hang-6",
      "cm-md-hang-5", null,
    ]);
    expect(view.lines().map((line) => line.textContent)).toEqual(
      ["\u2022 bullet", "* star", "+ plus", "1. first", "10) tenth", "  \u2022 nested", "plain"],
    );
    expect(view.editor.value()).toBe(text);
  });

  it("keeps markers on the paragraph baseline without inheriting the hanging indent", () => {
    const text = "Paragraph\n\n- hyphen\n* star\n+ plus\n1. number\n10) number\n  - nested";
    const view = render(text);
    expect(view.rule(".cm-line")).toContain("padding: 0 7px;");
    expect(view.rule(".cm-md-list-mark")).toContain("text-indent: 0;");
    expect(view.rule(".cm-md-bullet")).toContain("text-align: left;");
    for (const half of [3, 4, 5, 6]) {
      const rule = view.rule(`.cm-md-hang-${half}`);
      expect(rule).toContain(`padding-left: calc(7px + ${half / 2}ch);`);
      expect(rule).toContain(`text-indent: -${half / 2}ch;`);
    }
    expect(view.lines()[0]!.className).toBe("cm-line");
    expect(view.lines().every((line) => !line.hasAttribute("style"))).toBe(true);
    expect(view.editor.value()).toBe(text);
  });

  it("copies the source Markdown rather than bullet widgets", () => {
    const text = "- bullet #tag\n  - nested";
    const { editor } = render(text);
    editor.setReadOnly(false);
    editor.focus();
    editor.view.dispatch({ selection: { anchor: 0, head: text.length } });
    const setData = vi.fn();
    const copy = new Event("copy", { bubbles: true, composed: true, cancelable: true });
    Object.defineProperty(copy, "clipboardData", { value: { clearData: vi.fn(), setData } });
    editor.view.contentDOM.dispatchEvent(copy);
    expect(setData).toHaveBeenCalledExactlyOnceWith("text/plain", text);
    expect(editor.value()).toBe(text);
  });

  it("does not replace prose, thematic breaks, or hyphens in code", () => {
    const text = "a - word\n\n---\n\n`- inline`\n\n```\n- fenced\n```\n\n    - indented";
    const view = render(text);
    expect(view.texts(".cm-md-bullet")).toEqual([]);
    expect(view.editor.value()).toBe(text);
  });

  it("highlights nested, Unicode, and emoji tags without changing text", () => {
    const tags = [
      "#work", "#work/project", "#snake_case", "#kebab-case", "#y1984", "#caf\u00e9",
      "#\u4e2d\u6587", "#\u{1f4a1}", "#\u{1f469}\u200d\u{1f4bb}",
    ];
    const text = `Tags ${tags.join(" ")} (#bracket), #last.`;
    const view = render(text);
    expect(view.texts(".cm-md-tag")).toEqual([...tags, "#bracket", "#last"]);
    expect(view.editor.value()).toBe(text);
  });

  it("excludes headings, code, escapes, numeric tags, references, and URL fragments", () => {
    const text = [
      "# Heading #hidden", "", "Setext #hidden", "===", "", "`#inline`", "",
      "```", "#fenced", "```", "", "    #indented", "",
      "Words \\#escaped word#embedded #1984 #\u0661\u0662\u0663",
      "[[Note#heading]] [[#heading]] [[#unfinished", "",
      '[label](https://example.test/#destination "#title")',
      "https://example.test/?q=#fragment www.example.test/?q=#fragment", "#valid",
    ].join("\n");
    const view = render(text);
    expect(view.texts(".cm-md-tag")).toEqual(["#valid"]);
  });

  it("highlights tags inside emphasis without coloring its closing markers", () => {
    const view = render("Tags _#italic_ __#strong__ **#bold** _#1984_");
    expect(view.texts(".cm-md-tag")).toEqual(["#italic", "#strong", "#bold"]);
    expect(view.texts(".cm-md-mark")).toEqual(["_", "_", "__", "__", "**", "**", "_", "_"]);
  });

  it("refreshes tags and bullet widgets when their source is edited", () => {
    const view = render("- #before");
    expect(view.texts(".cm-md-bullet")).toEqual(["\u2022"]);
    expect(view.texts(".cm-md-tag")).toEqual(["#before"]);
    view.editor.setValue("* #after");
    expect(view.texts(".cm-md-bullet")).toEqual(["*"]);
    expect(view.texts(".cm-md-tag")).toEqual(["#after"]);
    view.editor.setValue("plain text");
    expect(view.texts(".cm-md-bullet, .cm-md-tag")).toEqual([]);
  });

  it("allows deleting a rendered marker and undoing it without changing the draft history", () => {
    const view = render("- item");
    view.editor.setReadOnly(false);
    view.editor.focus();
    view.editor.view.dispatch({ selection: { anchor: 1 } });
    view.editor.view.contentDOM.dispatchEvent(new KeyboardEvent("keydown", {
      key: "Backspace", bubbles: true, composed: true, cancelable: true,
    }));
    expect(view.editor.value()).toBe(" item");
    expect(view.texts(".cm-md-bullet")).toEqual([]);
    expect(undo(view.editor.view)).toBe(true);
    expect(view.editor.value()).toBe("- item");
    expect(view.texts(".cm-md-bullet")).toEqual(["\u2022"]);
    expect(undo(view.editor.view)).toBe(false);
  });

  it("leaves other Markdown unstyled", () => {
    const view = render("# Heading\n~~strike~~ `code` ==mark==\nsnake_case_word > quote");
    expect(view.texts("[class*='cm-md-']")).toEqual([]);
  });

  describe("list Enter", () => {
    let editor: JournalEditor;
    afterEach(() => {
      editor?.destroy();
      document.body.innerHTML = "";
    });

    function setup(text: string, anchor = text.length, head = anchor) {
      const host = document.createElement("div");
      document.body.append(host);
      editor = createEditor(host, {
        placeholder: "", label: "Journal entry", onChange() {}, notes: () => [],
      });
      editor.setValue(text);
      editor.setReadOnly(false);
      editor.view.dispatch({ selection: { anchor, head } });
      return editor;
    }

    it.each([
      ["- item", "- item\n- "],
      ["* item", "* item\n* "],
      ["+ item", "+ item\n+ "],
      ["1. item", "1. item\n2. "],
      ["9) item", "9) item\n10) "],
      ["-  item", "-  item\n-  "],
      ["- parent\n  - child", "- parent\n  - child\n  - "],
      ["- parent\n  - child\n    detail", "- parent\n  - child\n    detail\n  - "],
      ["1. parent\n   9) child", "1. parent\n   9) child\n   10) "],
      ["- parent\n\t- child", "- parent\n\t- child\n\t- "],
      ["- - child", "- - child\n  - "],
      ["> - quoted", "> - quoted\n> - "],
      ["- item\n  continued", "- item\n  continued\n- "],
      ["10) item\n    continued", "10) item\n    continued\n11) "],
      ["> - item\n>   continued", "> - item\n>   continued\n> - "],
      ["- item\n\n- new item", "- item\n\n- new item\n- "],
      ["- `inline code`", "- `inline code`\n- "],
    ])("continues %j", (text, expected) => {
      setup(text);
      expect(continueList(editor)).toBe(true);
      expect(editor.value()).toBe(expected);
      expect(editor.view.state.selection.main.head).toBe(expected.length);
    });

    it.each([
      ["- item\n- ", "- item\n\n"],
      ["1. item\n2. ", "1. item\n\n"],
      ["- parent\n  - child\n  - ", "- parent\n  - child\n\n"],
      ["- item\n- \t ", "- item\n\n"],
      ["- ", "\n"],
      ["- item\n-", "- item\n\n"],
    ])("exits the empty item in %j", (text, expected) => {
      setup(text);
      expect(continueList(editor)).toBe(true);
      expect(editor.value()).toBe(expected);
      expect(editor.view.state.selection.main.head).toBe(expected.length);
    });

    it.each([
      "plain", "a - word", "---", "# heading", "- item\nparagraph", "- item\n",
      "- item\n  ", "- item\n\n  paragraph", "- item\n \t \n  paragraph",
      "```\n- code\n```", "    - code", "- item\n  ```\n  - code",
    ])("does not continue outside a list or across a gap in %j", (text) => {
      setup(text);
      expect(continueList(editor)).toBe(false);
      expect(editor.value()).toBe(text);
    });

    it("splits an item at the cursor and preserves the suffix", () => {
      setup("- first second", 7);
      expect(continueList(editor)).toBe(true);
      expect(editor.value()).toBe("- first\n-  second");
      expect(editor.view.state.selection.main.head).toBe(10);
    });

    it("replaces selected text using one undoable transaction", () => {
      setup("- first second", 3, 7);
      expect(continueList(editor)).toBe(true);
      expect(editor.value()).toBe("- f\n-  second");
      expect(undo(editor.view)).toBe(true);
      expect(editor.value()).toBe("- first second");
      expect(undo(editor.view)).toBe(false);
    });

    it("does not edit a read-only list", () => {
      setup("- item");
      editor.setReadOnly(true);
      expect(continueList(editor)).toBe(false);
      expect(editor.value()).toBe("- item");
    });
  });
});
