import { describe, expect, it } from "vitest";
import { countWords } from "./word-count";

describe("countWords", () => {
  it.each([
    ["empty text", "", 0],
    ["whitespace", " \t\r\n\u00a0\u2003 ", 0],
    ["punctuation and emoji", "... !? \u2014 \u3002 \u{1f600}\u{1f469}\u200d\u{1f4bb}", 0],
    ["standalone Markdown markers", "- # ** __ --- > [[ ]] ```", 0],
    ["multiline prose", "  one\t two\nthree\r\nfour  ", 4],
    ["punctuation-separated words", "hello,world! file.txt snake_case a/b", 8],
    ["numbers", "2026 42 abc123", 3],
    ["contractions and hyphens", "don't don\u2019t long-term mother-in-law", 4],
    ["external apostrophes and repeated hyphens", "'hello' \u2019world\u2019 one--two", 4],
    ["accented and decomposed words", "caf\u00e9 nai\u0308ve e\u0301lan", 3],
    ["standalone combining marks", "\u0301 \u0308", 0],
    ["non-Latin alphabets", "\u041f\u0440\u0438\u0432\u0435\u0442 \u03ba\u03cc\u03c3\u03bc\u03b5 \u0645\u0631\u062d\u0628\u0627", 3],
    ["Chinese", "\u4f60\u597d\u4e16\u754c", 4],
    ["Japanese", "\u65e5\u672c\u8a9e\u304b\u306a\u30ab\u30ca", 7],
    ["Korean", "\ud55c\uad6d\uc5b4", 3],
    ["CJK punctuation and combining marks", "\u4f60\uff0c\u597d\u3002\u304b\u3099", 3],
    ["supplementary-plane Han", "\u{20000}\u{20001}", 2],
    ["mixed scripts without spaces", "hello\u4f60\u597dworld", 4],
    ["mixed scripts with punctuation", "Hello \u65e5\u672c\u8a9e, world!", 5],
    ["Markdown content", "# Hello **world**\n- don't forget #work", 5],
    ["source rather than rendered Markdown", "[label](https://example.com) [[Note|alias]] `code`", 7],
  ])("counts %s", (_name, text, expected) => {
    expect(countWords(text)).toBe(expected);
  });
});
