import input from "rts:input";

// RTS-MINE engine — Input estilo Unity, preso a uma janela.
//
// Códigos de tecla ficam como LITERAIS aqui dentro (limite #1726: const
// importada como argumento dentro de método não captura). Métodos semânticos
// (axisH/axisV/jump) fazem o papel do Input.GetAxis do Unity.

export class Input {
  win: i64;
  constructor(win: i64) {
    this.win = win;
  }

  // ── cru ─────────────────────────────────────────────────────────────────────
  down(key: number): number {
    return input.key(this.win, key, 0);
  }
  pressed(key: number): number {
    return input.key(this.win, key, 1);
  }

  // ── eixos estilo Unity (-1 / 0 / +1) ────────────────────────────────────────
  /// A/D → strafe. (A=100, D=103)
  axisH(): number {
    const d = input.key(this.win, 103, 0);
    const a = input.key(this.win, 100, 0);
    return d - a;
  }
  /// W/S → frente/trás. (W=122, S=118)
  axisV(): number {
    const w = input.key(this.win, 122, 0);
    const s = input.key(this.win, 118, 0);
    return w - s;
  }
  /// Setas ←/→ (olhar alternativo). (esq=7, dir=8)
  arrowH(): number {
    const r = input.key(this.win, 8, 0);
    const l = input.key(this.win, 7, 0);
    return r - l;
  }
  /// Setas ↑/↓. (cima=5, baixo=6)
  arrowV(): number {
    const u = input.key(this.win, 5, 0);
    const d = input.key(this.win, 6, 0);
    return u - d;
  }
  /// Espaço segurado (pulo/nado). (espaço=3)
  jump(): number {
    return input.key(this.win, 3, 0);
  }
  /// Esc pressionado neste frame. (esc=2)
  quitPressed(): number {
    return input.key(this.win, 2, 1);
  }

  // ── mouse ───────────────────────────────────────────────────────────────────
  mouseX(): f64 {
    return input.mouseX(this.win);
  }
  mouseY(): f64 {
    return input.mouseY(this.win);
  }
  mouseDX(): f64 {
    return input.mouseDeltaX(this.win);
  }
  mouseDY(): f64 {
    return input.mouseDeltaY(this.win);
  }
  clickLeft(): number {
    return input.mouseClicked(this.win, 0);
  }
  clickRight(): number {
    return input.mouseClicked(this.win, 1);
  }
  wheel(): f64 {
    return input.wheel(this.win);
  }
  /// Ícone de crosshair do SO sobre a janela (chamar 1×/frame).
  crosshairCursor(): void {
    input.setCursor(this.win, 7);
  }
  /// Cursor padrão (menus).
  defaultCursor(): void {
    input.setCursor(this.win, 0);
  }
  /// Cursor de mão (hover em botão).
  pointerCursor(): void {
    input.setCursor(this.win, 1);
  }
}
