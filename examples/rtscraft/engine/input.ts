import { mouseX, mouseY, mouseClicked, mousePressed, mouseDeltaX, mouseDeltaY, wheel, key, setCursor } from "rts:input";

// Input estilo Unity, preso a uma janela. As leituras são capturadas uma vez no
// início do frame; gameplay e render consultam o snapshot local depois disso.
export class Input {
  win: i64;
  mx: f64;
  my: f64;
  mdx: f64;
  mdy: f64;
  whl: f64;
  leftClick: number;
  rightClick: number;
  wDown: number;
  aDown: number;
  sDown: number;
  dDown: number;
  upDown: number;
  downDown: number;
  leftDown: number;
  rightDown: number;
  jumpDown: number;
  escPressed: number;
  k1Pressed: number;
  k2Pressed: number;
  k3Pressed: number;
  k4Pressed: number;
  k5Pressed: number;

  constructor(win: i64) {
    this.win = win;
    this.mx = -1.0;
    this.my = -1.0;
    this.mdx = 0.0;
    this.mdy = 0.0;
    this.whl = 0.0;
    this.leftClick = 0;
    this.rightClick = 0;
    this.wDown = 0;
    this.aDown = 0;
    this.sDown = 0;
    this.dDown = 0;
    this.upDown = 0;
    this.downDown = 0;
    this.leftDown = 0;
    this.rightDown = 0;
    this.jumpDown = 0;
    this.escPressed = 0;
    this.k1Pressed = 0;
    this.k2Pressed = 0;
    this.k3Pressed = 0;
    this.k4Pressed = 0;
    this.k5Pressed = 0;
  }

  capture(): void {
    this.mx = mouseX(this.win);
    this.my = mouseY(this.win);
    this.mdx = mouseDeltaX(this.win);
    this.mdy = mouseDeltaY(this.win);
    this.whl = wheel(this.win);
    this.leftClick = mouseClicked(this.win, 0);
    this.rightClick = mouseClicked(this.win, 1);
    this.wDown = key(this.win, 122, 0);
    this.aDown = key(this.win, 100, 0);
    this.sDown = key(this.win, 118, 0);
    this.dDown = key(this.win, 103, 0);
    this.upDown = key(this.win, 5, 0);
    this.downDown = key(this.win, 6, 0);
    this.leftDown = key(this.win, 7, 0);
    this.rightDown = key(this.win, 8, 0);
    this.jumpDown = key(this.win, 3, 0);
    this.escPressed = key(this.win, 2, 1);
    this.k1Pressed = key(this.win, 131, 1);
    this.k2Pressed = key(this.win, 132, 1);
    this.k3Pressed = key(this.win, 133, 1);
    this.k4Pressed = key(this.win, 134, 1);
    this.k5Pressed = key(this.win, 135, 1);
  }

  down(code: number): number {
    if (code === 122) return this.wDown;
    if (code === 100) return this.aDown;
    if (code === 118) return this.sDown;
    if (code === 103) return this.dDown;
    if (code === 5) return this.upDown;
    if (code === 6) return this.downDown;
    if (code === 7) return this.leftDown;
    if (code === 8) return this.rightDown;
    if (code === 3) return this.jumpDown;
    return key(this.win, code, 0);
  }

  pressed(code: number): number {
    if (code === 2) return this.escPressed;
    if (code === 131) return this.k1Pressed;
    if (code === 132) return this.k2Pressed;
    if (code === 133) return this.k3Pressed;
    if (code === 134) return this.k4Pressed;
    if (code === 135) return this.k5Pressed;
    return key(this.win, code, 1);
  }

  axisH(): number { return this.dDown - this.aDown; }
  axisV(): number { return this.wDown - this.sDown; }
  arrowH(): number { return this.rightDown - this.leftDown; }
  arrowV(): number { return this.upDown - this.downDown; }
  jump(): number { return this.jumpDown; }
  quitPressed(): number { return this.escPressed; }
  mouseX(): f64 { return this.mx; }
  mouseY(): f64 { return this.my; }
  mouseDX(): f64 { return this.mdx; }
  mouseDY(): f64 { return this.mdy; }
  clickLeft(): number { return this.leftClick; }
  clickRight(): number { return this.rightClick; }
  wheel(): f64 { return this.whl; }
  crosshairCursor(): void { setCursor(this.win, 7); }
  defaultCursor(): void { setCursor(this.win, 0); }
  pointerCursor(): void { setCursor(this.win, 1); }
}
