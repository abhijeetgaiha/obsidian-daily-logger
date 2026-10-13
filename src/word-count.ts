const LETTER_OR_NUMBER = /[\p{L}\p{N}]/u;
const MARK = /\p{M}/u;
const CJK = /[\p{Script_Extensions=Han}\p{Script_Extensions=Hiragana}\p{Script_Extensions=Katakana}\p{Script_Extensions=Hangul}]/u;
const CONNECTOR = /['\u2019-]/u;

export function countWords(text: string): number {
  let count = 0;
  let inWord = false;
  let connector = false;
  for (const char of text) {
    if (LETTER_OR_NUMBER.test(char)) {
      if (CJK.test(char)) {
        count++;
        inWord = false;
      } else {
        if (!inWord) count++;
        inWord = true;
      }
      connector = false;
    } else if (MARK.test(char) && inWord && !connector) {
      continue;
    } else if (CONNECTOR.test(char) && inWord && !connector) {
      connector = true;
    } else {
      inWord = false;
      connector = false;
    }
  }
  return count;
}
