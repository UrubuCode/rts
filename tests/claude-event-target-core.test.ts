import { describe, test, expect } from "rts:test";

describe("núcleo geral EventTarget/Event", () => {
  test("dispatchEvent usa Event, cancelamento e passive", () => {
    const target: any = new EventTarget();
    let passivePrevented = true;
    let activePrevented = false;
    target.addEventListener("cancel", (event: any) => {
      event.preventDefault();
      passivePrevented = event.defaultPrevented;
    }, { passive: true });
    target.addEventListener("cancel", (event: any) => {
      event.preventDefault();
      activePrevented = event.defaultPrevented;
    });

    const event: any = new Event("cancel", { cancelable: true });
    const accepted = target.dispatchEvent(event);

    expect(accepted).toBe(false);
    expect(passivePrevented).toBe(false);
    expect(activePrevented).toBe(true);
    expect(event.defaultPrevented).toBe(true);
    expect(event.isTrusted).toBe(false);
  });

  test("CustomEvent herda de Event e preserva detail", () => {
    const target: any = new EventTarget();
    let detail = 0;
    target.addEventListener("custom", (event: any) => {
      detail = event.detail;
      expect(event instanceof Event).toBe(true);
    });

    const event: any = new CustomEvent("custom", { detail: 42 });
    expect(target.dispatchEvent(event)).toBe(true);
    expect(event.detail).toBe(42);
    expect(detail).toBe(42);
  });
});
