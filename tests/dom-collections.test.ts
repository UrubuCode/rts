import { describe, test, expect } from "rts:test";

const doc = parseDocument("<div id='x' class='a b' data-user-id='7'></div>");
const el = doc.getElementById("x");
const lista = el === null ? null : el.classList;
const dataset = el === null ? null : el.dataset;
if (el !== null && lista !== null && dataset !== null) {
  lista.add("c", "d");
  lista.remove("a");
  dataset.userId = "8";
  delete dataset.userId;
  dataset.userName = "Ada";
}

describe("DOM collections", () => {
  test("classList is live and supports variadic operations", () => {
    expect(lista === (el === null ? null : el.classList)).toBe(true);
    expect(lista.value).toBe("b c d");
    expect(lista.length).toBe(3);
    expect(lista.item(1)).toBe("c");
  });
  test("dataset maps camelCase to data-* and delete", () => {
    expect(dataset === (el === null ? null : el.dataset)).toBe(true);
    expect(dataset.userId).toBe("");
    expect(dataset.userName).toBe("Ada");
  });
});
