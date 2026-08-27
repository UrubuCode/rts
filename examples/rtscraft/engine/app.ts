import { openWindow, setNextWindowPos, pump, isOpen, close, mouseLock, beginFrame, endFrame, drawRect, drawText, drawLine, drawImage, winWidth, winHeight } from "rts:egui";
import { mouseX, mouseY, mouseClicked, mousePressed, mouseReleased, mouseDeltaX, mouseDeltaY, wheel, key, setCursor } from "rts:input";

// Adaptador do loop/UI imediatos do rtscraft para a superfície actual do RTS.
// A antiga fachada rts:render/rts:time/rts:io foi removida; este wrapper mantém a
// API ergonómica do jogo sem reintroduzir os namespaces históricos.
export class App {
  _win: i64;
  _lastMs: f64;
  _dt: f64;
  _fpsNow: f64;
  _active: number;

  constructor(title: string, w: number, h: number, x: number, y: number) {
    const win = openWindow(title, w, h, 0);
    this._win = win;
    this._lastMs = performance.now();
    this._dt = 0.0;
    this._fpsNow = 0.0;
    this._active = -1;
  }

  running(): number {
    if (this._win <= 0) return 0;
    return isOpen(this._win);
  }

  beginFrame(): number {
    if (this._win <= 0) return 0;
    const ok = pump(this._win);
    if (ok === 0 || isOpen(this._win) === 0) return 0;
    const now = performance.now();
    this._dt = now - this._lastMs;
    this._lastMs = now;
    if (this._dt < 0) this._dt = 0;
    if (this._dt > 0) this._fpsNow = 1000.0 / this._dt;
    beginFrame(this._win);
    return 1;
  }

  delta(): f64 {
    return this._dt;
  }

  fps(): f64 {
    return this._fpsNow;
  }

  endFrame(): void {
    if (this._win > 0) endFrame(this._win);
  }

  close(): void {
    if (this._win > 0) close(this._win);
  }

  keyPressed(code: number): number {
    return key(this._win, code, 1);
  }

  keyDown(code: number): number {
    return key(this._win, code, 0);
  }

  mouseX(): f64 {
    return mouseX(this._win);
  }

  mouseY(): f64 {
    return mouseY(this._win);
  }

  mouseDX(): f64 {
    return mouseDeltaX(this._win);
  }

  mouseDY(): f64 {
    return mouseDeltaY(this._win);
  }

  clickLeft(): number {
    return mouseClicked(this._win, 0);
  }

  clickRight(): number {
    return mouseClicked(this._win, 1);
  }

  wheel(): f64 {
    return wheel(this._win);
  }

  line(x1: number, y1: number, x2: number, y2: number, width: number, color: number): void {
    drawLine(this._win, { x1: x1, y1: y1, x2: x2, y2: y2, w: width, color: color });
  }

  box(x: number, y: number, w: number, h: number, fill: number, strokeW: number, stroke: number, radius: number): void {
    drawRect(this._win, { x: x, y: y, w: w, h: h, fill: fill, strokeW: strokeW, stroke: stroke, radius: radius });
  }

  text(x: number, y: number, content: string, color: number, size: number): void {
    drawText(this._win, { x: x, y: y, text: content, color: color, size: size });
  }

  image(x: number, y: number, w: number, h: number, pixels: Uint8Array, imgW: number, imgH: number): void {
    drawImage(this._win, { x: x, y: y, w: w, h: h, pixels: pixels, imgWidth: imgW, imgHeight: imgH });
  }

  clickable(id: number, x: number, y: number, w: number, h: number): number {
    const mx = mouseX(this._win);
    const my = mouseY(this._win);
    const over = mx >= x && mx <= x + w && my >= y && my <= y + h;
    if (over && mousePressed(this._win, 0) !== 0) this._active = id;
    let result = 0;
    if (over) result = 1;
    if (this._active === id) {
      result = 2;
      if (mouseReleased(this._win, 0) !== 0) {
        if (over) result = 3;
        this._active = -1;
      }
    }
    return result;
  }

  setMouseLock(on: number): void {
    if (this._win > 0) mouseLock(this._win, on);
  }

  setCrosshairCursor(): void {
    setCursor(this._win, 7);
  }

  setDefaultCursor(): void {
    setCursor(this._win, 0);
  }

  width(): number {
    return winWidth(this._win);
  }

  height(): number {
    return winHeight(this._win);
  }
}

export function createAppAt(title: string, w: number, h: number, x: number, y: number): App {
  setNextWindowPos(x, y);
  return new App(title, w, h, x, y);
}
