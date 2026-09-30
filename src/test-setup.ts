// jsdom has no layout or shadow-root selection; these stubs let CodeMirror run in tests.

const emptyRects = () => {
  const rects: DOMRect[] = [];
  return Object.assign(rects, { item: () => null }) as unknown as DOMRectList;
};
const emptyRect = () => new DOMRect(0, 0, 0, 0);

Range.prototype.getClientRects = emptyRects;
Range.prototype.getBoundingClientRect = emptyRect;

if (!("getSelection" in ShadowRoot.prototype)) {
  Object.defineProperty(ShadowRoot.prototype, "getSelection", {
    value: () => document.getSelection(),
  });
}

document.elementFromPoint ??= () => null;
// jsdom's navigator.vendor makes CodeMirror take its Safari shadow-root path, which needs this.
document.execCommand ??= () => false;
