import { describe, test, expect } from "rts:test";
import dom from "rts:dom";

function makeInput(value: string): { doc: Document; field: Element } {
  const doc = parseDocument("<body><input id='field' value='" + value + "'></body>");
  const field = doc.getElementById("field");
  if (field === null) throw new Error("input de teste não encontrado");
  dom.focusInput(doc._dom, field.nodeId);
  return { doc, field };
}

describe("eventos de edição do DOM", () => {
  test("faz beforeinput antes da mutação e input depois, com metadados", () => {
    const { doc, field } = makeInput("a");
    let order = "";
    let beforeData = "";
    let beforeType = "";
    let beforeTrusted = false;
    let inputValue = "";
    let inputCancelable = true;
    field.addEventListener("beforeinput", (event: any) => {
      order = order + "b";
      beforeData = event.data;
      beforeType = event.inputType;
      beforeTrusted = event.isTrusted;
      inputValue = dom.inputValue(doc._dom, field.nodeId);
    });
    field.addEventListener("input", (event: any) => {
      order = order + "i";
      inputValue = dom.inputValue(doc._dom, field.nodeId);
      inputCancelable = event.cancelable;
    });

    dom.pushRawTextInput(doc._dom, "x");
    const pumped = pumpInputEvents(doc);

    expect(pumped).toBe(1);
    expect(order).toBe("bi");
    expect(beforeData).toBe("x");
    expect(beforeType).toBe("insertText");
    expect(beforeTrusted).toBe(true);
    expect(inputValue).toBe("ax");
    expect(inputCancelable).toBe(false);
  });

  test("preventDefault em beforeinput bloqueia a mutação e input", () => {
    const { doc, field } = makeInput("a");
    let inputEvents = 0;
    field.addEventListener("beforeinput", (event: any) => event.preventDefault());
    field.addEventListener("input", (_event: any) => inputEvents = inputEvents + 1);

    dom.pushRawTextInput(doc._dom, "x");
    pumpInputEvents(doc);

    expect(dom.inputValue(doc._dom, field.nodeId)).toBe("a");
    expect(inputEvents).toBe(0);
  });

  test("backspace é uma acção padrão cancelável depois de keydown", () => {
    const { doc, field } = makeInput("ab");
    let order = "";
    field.addEventListener("keydown", (_event: any) => order = order + "k");
    field.addEventListener("beforeinput", (event: any) => {
      order = order + "b";
      expect(event.inputType).toBe("deleteContentBackward");
      expect(event.data).toBe("");
    });
    field.addEventListener("input", (_event: any) => order = order + "i");

    dom.pushRawKeyboardEvent(doc._dom, 4, 1);
    const pumped = pumpInputEvents(doc);

    expect(pumped).toBe(1);
    expect(order).toBe("kbi");
    expect(dom.inputValue(doc._dom, field.nodeId)).toBe("a");
  });

  test("preventDefault em keydown bloqueia beforeinput e backspace", () => {
    const { doc, field } = makeInput("ab");
    let beforeEvents = 0;
    field.addEventListener("keydown", (event: any) => event.preventDefault());
    field.addEventListener("beforeinput", (_event: any) => beforeEvents = beforeEvents + 1);

    dom.pushRawKeyboardEvent(doc._dom, 4, 1);
    pumpInputEvents(doc);

    expect(dom.inputValue(doc._dom, field.nodeId)).toBe("ab");
    expect(beforeEvents).toBe(0);
  });

  test("eventos backend são trusted e dispatchEvent sintético não é", () => {
    const { doc, field } = makeInput("");
    let backendTrusted = true;
    let syntheticTrusted = true;
    field.addEventListener("input", (event: any) => {
      if (event.isTrusted) backendTrusted = true;
      else syntheticTrusted = false;
    });

    dom.pushRawTextInput(doc._dom, "x");
    pumpInputEvents(doc);
    field.dispatchEvent("input");

    expect(backendTrusted).toBe(true);
    expect(syntheticTrusted).toBe(false);
  });

  test("composição entrega start/update/end e insere o commit uma só vez", () => {
    const { doc, field } = makeInput("");
    let order = "";
    let updateData = "";
    let updateComposing = false;
    let endData = "";
    let endComposing = true;
    let committedInput = 0;
    field.addEventListener("compositionstart", (event: any) => {
      order = order + "s";
      expect(event.isTrusted).toBe(true);
      expect(event.isComposing).toBe(false);
    });
    field.addEventListener("compositionupdate", (event: any) => {
      order = order + "u";
      updateData = event.data;
      updateComposing = event.isComposing;
    });
    field.addEventListener("compositionend", (event: any) => {
      order = order + "e";
      endData = event.data;
      endComposing = event.isComposing;
    });
    field.addEventListener("beforeinput", (event: any) => {
      order = order + "b";
      expect(event.inputType).toBe("insertText");
      expect(event.isComposing).toBe(false);
    });
    field.addEventListener("input", (_event: any) => {
      order = order + "i";
      committedInput = committedInput + 1;
    });

    dom.pushRawCompositionEvent(doc._dom, 2, "");
    pumpInputEvents(doc);
    dom.pushRawCompositionEvent(doc._dom, 3, "ka");
    pumpInputEvents(doc);
    dom.pushRawCompositionEvent(doc._dom, 4, "か");
    dom.pushRawTextInput(doc._dom, "か");
    pumpInputEvents(doc);

    expect(order).toBe("suebi");
    expect(updateData).toBe("ka");
    expect(updateComposing).toBe(true);
    expect(endData).toBe("か");
    expect(endComposing).toBe(false);
    expect(committedInput).toBe(1);
    expect(dom.inputValue(doc._dom, field.nodeId)).toBe("か");
  });

  test("Disabled encerra composição sem criar texto", () => {
    const { doc, field } = makeInput("");
    let starts = 0;
    let ends = 0;
    field.addEventListener("compositionstart", (_event: any) => starts = starts + 1);
    field.addEventListener("compositionend", (event: any) => {
      ends = ends + 1;
      expect(event.data).toBe("");
    });

    dom.pushRawCompositionEvent(doc._dom, 2, "");
    dom.pushRawCompositionEvent(doc._dom, 5, "");
    pumpInputEvents(doc);

    expect(starts).toBe(1);
    expect(ends).toBe(1);
    expect(dom.inputValue(doc._dom, field.nodeId)).toBe("");
  });
});
