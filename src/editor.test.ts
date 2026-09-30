import { afterEach, describe, expect, it } from "vitest";
import type { NoteLink } from "./api";
import { createEditor, MAX_SUGGESTIONS, rankNotes, type JournalEditor } from "./editor";

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
      texts: (selector: string) =>
        [...shadow.querySelectorAll(selector)].map((element) => element.textContent),
      lines: () => [...shadow.querySelectorAll<HTMLElement>(".cm-line")],
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
    const view = render("- bullet\n* star\n1. first\n10) tenth\n  - nested\nplain");
    expect(view.texts(".cm-md-bullet")).toEqual(["-", "*", "-"]);
    expect(view.texts(".cm-md-number")).toEqual(["1.", "10)"]);
    const hangs = view.lines().map((line) =>
      [...line.classList].find((name) => name.startsWith("cm-md-hang-")) ?? null);
    expect(hangs).toEqual([
      "cm-md-hang-3", "cm-md-hang-3", "cm-md-hang-4", "cm-md-hang-6", "cm-md-hang-5", null,
    ]);
    expect(view.lines().map((line) => line.textContent)).toEqual(
      ["- bullet", "* star", "1. first", "10) tenth", "  - nested", "plain"],
    );
  });

  it("leaves other Markdown unstyled", () => {
    const view = render("# Heading\n~~strike~~ `code` ==mark==\nsnake_case_word > quote");
    expect(view.texts("[class*='cm-md-']")).toEqual([]);
  });
});
