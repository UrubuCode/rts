// `rts:canvas` — API ergonômica de UI IMEDIATA sobre a interface abstrata
// `render.*` + `input.*`. Roda FORA do DOM: um modo de desenhar UI direto no
// canvas (retângulos, texto, botões) sem árvore retida. O backend (egui hoje, ou
// qualquer outro Renderer registrado) só pinta — trocar o backend não muda este
// código. Ver docs/specs/dom-render-input-interfaces.md.
//
// Regras de design do motor (provadas empiricamente): propriedades são
// getters/setters; APIs que podem falhar são métodos de classe; campos internos
// (`_win`) só lidos via `this.`. A API DESENHA e CONSULTA INPUT — não guarda
// estado de árvore.

// `render.*` e `input.* ` são os namespaces abstratos. O `Canvas` os embrulha numa
// API amigável presa a UMA janela (`_win`).
class Canvas {
  _win: number; // handle da janela/alvo

  constructor(win: number) {
    this._win = win;
  }

  // ── ciclo de frame ───────────────────────────────────────────────────────────
  begin(): void {
    render.beginFrame(this._win);
  }
  end(): void {
    render.endFrame(this._win);
  }

  // ── primitivos de desenho (cor 0xRRGGBBAA) ───────────────────────────────────
  /// Retângulo preenchido simples.
  fillRect(x: number, y: number, w: number, h: number, color: number): void {
    render.rect(this._win, x, y, w, h, color, 0, 0, 0);
  }
  /// Retângulo com fundo + borda + cantos.
  box(x: number, y: number, w: number, h: number, fill: number, strokeW: number, stroke: number, radius: number): void {
    render.rect(this._win, x, y, w, h, fill, strokeW, stroke, radius);
  }
  /// Texto em (x,y) topo-esquerda.
  text(x: number, y: number, s: string, color: number, size: number): void {
    render.text(this._win, x, y, s, color, size, 0);
  }
  /// Linha.
  line(x1: number, y1: number, x2: number, y2: number, w: number, color: number): void {
    render.line(this._win, x1, y1, x2, y2, w, color);
  }
  /// Largura do texto na fonte real (para alinhar/centralizar).
  measure(s: string, size: number): number {
    return render.measureText(this._win, s, size, 0);
  }

  // ── input (consulta o estado cru do backend; ler DENTRO do frame) ─────────────
  mouseX(): number {
    return input.mouseX(this._win);
  }
  mouseY(): number {
    return input.mouseY(this._win);
  }
  /// `true` se o ponto do mouse está dentro do retângulo (hit-test).
  hover(x: number, y: number, w: number, h: number): boolean {
    const mx = input.mouseX(this._win);
    const my = input.mouseY(this._win);
    return mx >= x && mx <= x + w && my >= y && my <= y + h;
  }
  /// `true` se houve clique (botão esquerdo) neste frame.
  clicked(): boolean {
    return input.mouseClicked(this._win, 0) !== 0;
  }

  // ── widget de conveniência: BOTÃO (UI imediata) ───────────────────────────────
  /// Desenha um botão e retorna `true` se foi clicado NESTE frame. Hit-test +
  /// hover + clique embutidos — o estilo de UI imediata (como o egui), mas sobre a
  /// abstração render.*/input.* (backend-agnóstico).
  button(x: number, y: number, w: number, h: number, label: string): boolean {
    const over = this.hover(x, y, w, h);
    let fill = 0x2A3A50FF;
    if (over) fill = 0x3A4F6EFF;
    render.rect(this._win, x, y, w, h, fill, 2, 0x6699CCFF, 8);
    // centraliza o label
    const tw = render.measureText(this._win, label, 16, 0);
    const tx = x + (w - tw) / 2;
    render.text(this._win, tx, y + h / 2 - 9, label, 0xFFFFFFFF, 16, 0);
    return over && this.clicked();
  }
}

/// `createCanvas(win)` — embrulha um handle de janela num `Canvas` ergonômico.
function createCanvas(win: number): Canvas {
  return new Canvas(win);
}
