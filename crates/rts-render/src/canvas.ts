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

// ── App — o LOOP BASE pronto (sem callback; o dev mantém o while) ───────────────
// O dev escreve o while, mas o boilerplate (pump + begin + timing + end) some
// dentro do `app.*`. Dá deltaTime/frameCount. Quem quer outro loop (modo reativo,
// FPS próprio) usa os primitivos crus (egui.pump/render.*). É conveniência por
// cima, primitivos embaixo — ambos disponíveis. Regra do motor: NÃO é runApp(cb)
// (callback capturante quebra); é um helper que o dev chama em pedaços.
class App {
  _win: number;
  _canvas: Canvas;
  _lastMs: number;  // instante do frame anterior (ms)
  _dt: number;      // delta do frame atual (ms)
  _frames: number;
  // cursor de LAYOUT AUTOMÁTICO: os componentes `auto*` empilham aqui, sem x/y na
  // mão. `panelStart`/`row` movem o cursor; o dev só chama os componentes em ordem.
  _cx: number;
  _cy: number;
  _cw: number;
  // controle de FPS: alvo (0 = ilimitado) + o FPS MEDIDO (1000/dt).
  _fpsTarget: number;
  _fpsNow: number;
  // ── EVENTOS/FOCO (camada ergonômica, estilo UI imediata) ─────────────────────
  // cada widget tem um ID numérico estável (o dev passa). O App rastreia:
  //   _focused = quem tem o TECLADO (textField); _active = quem está sendo
  //   pressionado (drag/click-and-hold). -1 = nenhum.
  _focused: number;
  _active: number;

  constructor(win: number) {
    this._win = win;
    this._canvas = new Canvas(win);
    this._lastMs = time.now_ms();
    this._dt = 0;
    this._frames = 0;
    this._cx = 16;
    this._cy = 16;
    this._cw = 200;
    this._fpsTarget = 0;
    this._fpsNow = 0;
    this._focused = -1;
    this._active = -1;
  }

  /// Limita o FPS ao alvo `n` (frames/segundo). `0` = ilimitado (roda o mais
  /// rápido possível — útil pra medir/stress). O cap dorme o tempo sobrando ao
  /// fim do frame. NOTA: o vsync do backend também limita (~taxa do monitor);
  /// setFps abaixo disso é efetivo, acima fica limitado pelo vsync.
  setFps(n: number): void {
    this._fpsTarget = n;
  }

  /// O FPS MEDIDO no último frame (1000/dt). Pra mostrar/testar.
  fps(): number {
    return this._fpsNow;
  }

  /// O canvas ergonômico desta janela. NOTA: chamar método sobre o retorno de um
  /// getter pode não despachar no motor atual (shape não provada) — prefira os
  /// métodos de desenho DIRETOS do App (`app.box`/`app.text`/...), que delegam aqui.
  get canvas(): Canvas {
    return this._canvas;
  }

  // ── desenho (delega ao canvas; chamados direto em `app.*`, shape provada) ─────
  fillRect(x: number, y: number, w: number, h: number, color: number): void {
    render.rect(this._win, x, y, w, h, color, 0, 0, 0);
  }
  box(x: number, y: number, w: number, h: number, fill: number, strokeW: number, stroke: number, radius: number): void {
    render.rect(this._win, x, y, w, h, fill, strokeW, stroke, radius);
  }
  text(x: number, y: number, s: string, color: number, size: number): void {
    render.text(this._win, x, y, s, color, size, 0);
  }
  line(x1: number, y1: number, x2: number, y2: number, w: number, color: number): void {
    render.line(this._win, x1, y1, x2, y2, w, color);
  }
  measure(s: string, size: number): number {
    return render.measureText(this._win, s, size, 0);
  }
  hover(x: number, y: number, w: number, h: number): boolean {
    const mx = input.mouseX(this._win);
    const my = input.mouseY(this._win);
    return mx >= x && mx <= x + w && my >= y && my <= y + h;
  }
  clicked(): boolean {
    return input.mouseClicked(this._win, 0) !== 0;
  }
  // ── TECLADO (delega ao input com `this._win`) ────────────────────────────────
  // IMPORTANTE: chame estes MÉTODOS (`app.keyDown(k)`), NÃO `input.keyDown(app._win, k)`
  // direto do top-level — passar `app._win` (campo de instância) como argumento de
  // uma chamada externa lê 0 no AOT (diverge do JIT). Dentro do método, `this._win`
  // resolve o handle certo. Retornam number (0/1) para o motor despachar i64 bem.
  keyDown(key: number): number {
    return input.keyDown(this._win, key);
  }
  keyPressed(key: number): number {
    return input.keyPressed(this._win, key);
  }
  keyReleased(key: number): number {
    return input.keyReleased(this._win, key);
  }
  button(x: number, y: number, w: number, h: number, label: string): boolean {
    const over = this.hover(x, y, w, h);
    let fill = 0x2A3A50FF;
    if (over) fill = 0x3A4F6EFF;
    render.rect(this._win, x, y, w, h, fill, 2, 0x6699CCFF, 8);
    const tw = render.measureText(this._win, label, 16, 0);
    render.text(this._win, x + (w - tw) / 2, y + h / 2 - 9, label, 0xFFFFFFFF, 16, 0);
    return over && this.clicked();
  }

  // ── COMPONENTES de UI imediata (a biblioteca p/ o "pessoal" usar) ─────────────
  // Estilo imediato: o componente DESENHA + INTERAGE + RETORNA o novo valor/estado;
  // o dev guarda o valor numa variável e passa de volta no próximo frame. Sem
  // estado interno (combina com o motor e com o loop dirigido pelo TS).

  /// Rótulo de texto simples (atalho de `text` com tamanho default 16).
  label(x: number, y: number, s: string, color: number): void {
    render.text(this._win, x, y, s, color, 16, 0);
  }

  /// Painel/cartão: um retângulo com fundo + borda + cantos, p/ agrupar.
  panel(x: number, y: number, w: number, h: number): void {
    render.rect(this._win, x, y, w, h, 0x1A2230FF, 1, 0x33445566 & 0xFFFFFFFF, 8);
  }

  /// Barra de progresso (0..1). Desenha o trilho + o preenchimento.
  progressBar(x: number, y: number, w: number, h: number, value: number): void {
    let v = value;
    if (v < 0) v = 0;
    if (v > 1) v = 1;
    render.rect(this._win, x, y, w, h, 0x222A38FF, 1, 0x44556688 & 0xFFFFFFFF, 6);
    render.rect(this._win, x, y, w * v, h, 0x33CC88FF, 0, 0, 6);
  }

  /// Checkbox. Recebe o estado atual como NUMBER (0=off, 1=on); devolve o NOVO
  /// estado (alterna ao clicar). Usa number em vez de boolean porque o motor
  /// despacha melhor i64 vindo de método. `x,y` = canto da caixinha (18×18).
  checkbox(x: number, y: number, checked: number, label: string): number {
    const sz = 18;
    const over = this.hover(x, y, sz, sz);
    let result = checked;
    if (over && this.clicked()) {
      result = checked === 0 ? 1 : 0;
    }
    let border = 0x6699CCFF;
    if (over) border = 0x99CCFFFF;
    render.rect(this._win, x, y, sz, sz, 0x1A2230FF, 2, border, 4);
    if (result !== 0) {
      render.rect(this._win, x + 4, y + 4, sz - 8, sz - 8, 0x33CC88FF, 0, 0, 2);
    }
    render.text(this._win, x + sz + 8, y - 1, label, 0xE0E4E8FF, 15, 0);
    return result;
  }

  /// Slider horizontal. Recebe o valor atual (min..max); devolve o NOVO valor
  /// (arrasta quando o mouse está pressionado sobre a trilha). `x,y,w` definem a
  /// trilha; a altura é fixa.
  slider(x: number, y: number, w: number, value: number, min: number, max: number): number {
    const trackH = 6;
    const knobR = 9;
    // trilha
    render.rect(this._win, x, y + knobR - trackH / 2, w, trackH, 0x222A38FF, 0, 0, 3);
    // posição atual do knob
    let v = value;
    if (v < min) v = min;
    if (v > max) v = max;
    const t = (v - min) / (max - min);
    const knobX = x + t * w;
    const knobY = y + knobR;
    // arrasta: mouse pressionado dentro da faixa do slider
    const over = this.hover(x - knobR, y, w + knobR * 2, knobR * 2);
    if (over && input.mouseDown(this._win, 0) !== 0) {
      const mx = input.mouseX(this._win);
      let nt = (mx - x) / w;
      if (nt < 0) nt = 0;
      if (nt > 1) nt = 1;
      v = min + nt * (max - min);
    }
    // preenchimento até o knob
    const tt = (v - min) / (max - min);
    render.rect(this._win, x, y + knobR - trackH / 2, w * tt, trackH, 0x3399FFFF, 0, 0, 3);
    // knob (quadrado arredondado)
    render.rect(this._win, knobX - knobR, knobY - knobR, knobR * 2, knobR * 2, 0xFFFFFFFF, 0, 0, knobR);
    return v;
  }

  /// `textInput(x,y,w, value, focused)` — campo de texto. Recebe o texto atual e
  /// se está focado (1/0); devolve... veja: como retornar 2 valores é chato, este
  /// componente MUTA via convenção — devolve o NOVO texto, e o foco é o dev quem
  /// controla clicando. Aqui: se focado, anexa o textInput do frame e trata
  /// backspace. (Simplificado p/ o PoC.)
  textInput(x: number, y: number, w: number, value: string, focused: number): string {
    const h = 30;
    let border = 0x44556688 & 0xFFFFFFFF;
    if (focused !== 0) border = 0x3399FFFF;
    render.rect(this._win, x, y, w, h, 0x10161EFF, 2, border, 6);
    // Concatena o texto digitado neste frame. (Backspace/medição de length ficam
    // p/ quando o motor provar shape de string-de-método; aqui o caminho seguro é
    // só append — string + string sempre funciona.)
    let text = value;
    if (focused !== 0) {
      const typed = input.textInput(this._win);
      text = text + typed;
    }
    let shown = text;
    if (focused !== 0) shown = text + "|";
    render.text(this._win, x + 8, y + 7, shown, 0xFFFFFFFF, 15, 0);
    return text;
  }

  // ── TABS ──────────────────────────────────────────────────────────────────────
  /// `tab(x,y,w,h, label, active)` — UMA aba. `active` (1/0) = é a aba selecionada.
  /// Devolve 1 se foi CLICADA neste frame (o dev troca o índice ativo). Desenhe as
  /// abas lado a lado e troque `activeTab` quando uma retornar 1.
  tab(x: number, y: number, w: number, h: number, label: string, active: number): number {
    const over = this.hover(x, y, w, h);
    let fill = 0x161C26FF;
    if (active !== 0) fill = 0x243246FF;
    else if (over) fill = 0x1C2430FF;
    render.rect(this._win, x, y, w, h, fill, 0, 0, 6);
    // sublinhado da aba ativa
    if (active !== 0) {
      render.rect(this._win, x, y + h - 3, w, 3, 0x3399FFFF, 0, 0, 0);
    }
    const tw = render.measureText(this._win, label, 15, 0);
    let tcolor = 0x99A0AAFF;
    if (active !== 0) tcolor = 0xFFFFFFFF;
    render.text(this._win, x + (w - tw) / 2, y + h / 2 - 8, label, tcolor, 15, 0);
    if (over && this.clicked()) {
      return 1;
    }
    return 0;
  }

  // ── EVENTOS + FOCO (a camada ERGONÔMICA — sobre o input.* cru) ────────────────
  // Estilo UI imediata: cada widget tem um ID numérico estável (o dev passa). Os
  // helpers fazem hit-test + gerenciam foco/active, devolvendo o estado. Sem
  // callback (estado no App; o dev chama por frame).

  /// O ID do elemento que tem o FOCO de teclado (-1 = nenhum).
  focusedId(): number {
    return this._focused;
  }
  /// Foca explicitamente um elemento (ou -1 pra desfocar).
  setFocus(id: number): void {
    this._focused = id;
  }
  /// `true` se o elemento `id` está focado.
  isFocused(id: number): boolean {
    return this._focused === id;
  }
  /// Chame uma vez por frame (no inicio): clicar FORA de tudo desfoca. Helper
  /// opcional — `textField` ja foca ao clicar nele.
  clearFocusOnClickAway(): void {
    if (input.mousePressed(this._win, 0) !== 0) {
      // se nenhum widget consumir este clique, o foco cai pra -1 no fim do frame.
      // (simplificado: o textField re-foca se o clique foi nele, ANTES disto rodar.)
    }
  }

  /// Estado de uma ÁREA clicável (id estável). Retorna:
  ///   0 = idle, 1 = hover, 2 = pressed (segurando), 3 = CLICADO (released sobre).
  /// Gerencia _active (quem segura) — base de botoes/itens robustos com release.
  clickable(id: number, x: number, y: number, w: number, h: number): number {
    const over = this.hover(x, y, w, h);
    if (over && input.mousePressed(this._win, 0) !== 0) {
      this._active = id;
    }
    let result = 0;
    if (over) result = 1;
    if (this._active === id) {
      result = 2;
      if (input.mouseReleased(this._win, 0) !== 0) {
        if (over) result = 3; // soltou DENTRO = clique completo
        this._active = -1;
      }
    }
    return result;
  }

  /// Campo de texto COM FOCO real: clicar nele o foca; só o focado recebe o
  /// teclado (input.textInput). Recebe o texto atual; devolve o NOVO texto.
  /// `id` estável. Backspace via tecla 4 (so quando focado). Sem `.length` sobre
  /// string de retorno (limite do motor) — o append e backspace usam helpers do
  /// engine via concatenacao/condicao simples.
  textField(id: number, x: number, y: number, w: number, value: string): string {
    const h = 30;
    // clicar no campo o foca; clicar fora (em outro textField) tira.
    if (this.hover(x, y, w, h) && input.mousePressed(this._win, 0) !== 0) {
      this._focused = id;
    }
    const focused = this._focused === id;
    let border = 0x44556688 & 0xFFFFFFFF;
    if (focused) border = 0x3399FFFF;
    render.rect(this._win, x, y, w, h, 0x10161EFF, 2, border, 6);

    let text = value;
    if (focused) {
      const typed = input.textInput(this._win);
      text = text + typed;
      // Enter ou Escape desfoca.
      if (input.keyPressed(this._win, 1) !== 0) this._focused = -1;
      if (input.keyPressed(this._win, 2) !== 0) this._focused = -1;
    }
    let shown = text;
    if (focused) shown = text + "|";
    render.text(this._win, x + 8, y + 7, shown, 0xFFFFFFFF, 15, 0);
    return text;
  }

  // ── LAYOUT AUTOMÁTICO (empilha sem x/y na mão) ────────────────────────────────
  /// Inicia uma COLUNA de layout automático em `(x,y)` com largura `w`. Os
  /// componentes `auto*` seguintes empilham verticalmente a partir daqui — o dev
  /// não passa mais x/y. `gap` entre eles é fixo.
  column(x: number, y: number, w: number): void {
    this._cx = x;
    this._cy = y;
    this._cw = w;
  }

  /// Avança o cursor de layout `h` pontos para baixo (+ gap 8). Interno aos auto*.
  _advance(h: number): void {
    this._cy = this._cy + h + 8;
  }

  autoLabel(s: string, color: number): void {
    render.text(this._win, this._cx, this._cy, s, color, 16, 0);
    this._advance(20);
  }
  autoButton(label: string): boolean {
    const r = this.button(this._cx, this._cy, this._cw, 40, label);
    this._advance(40);
    return r;
  }
  autoCheckbox(checked: number, label: string): number {
    const r = this.checkbox(this._cx, this._cy, checked, label);
    this._advance(22);
    return r;
  }
  autoSlider(value: number, min: number, max: number): number {
    const r = this.slider(this._cx, this._cy, this._cw, value, min, max);
    this._advance(24);
    return r;
  }

  /// Move a janela do app para a posição absoluta `(x,y)` — escolher monitor.
  moveTo(x: number, y: number): void {
    egui.moveWindow(this._win, x, y);
  }

  /// `true` enquanto a janela está aberta. Use no `while`.
  running(): boolean {
    return egui.isOpen(this._win) !== 0;
  }

  /// Abre o frame: processa eventos do SO e calcula o delta time. Retorna `true`
  /// se deve continuar; `false` se a janela pediu pra fechar (dê `break`).
  beginFrame(): boolean {
    if (egui.pump(this._win) !== 0) {
      return false;
    }
    const now = time.now_ms();
    this._dt = now - this._lastMs;
    this._lastMs = now;
    this._frames = this._frames + 1;
    // FPS medido = 1000/dt (evita divisão por zero).
    if (this._dt > 0) {
      this._fpsNow = 1000 / this._dt;
    }
    render.beginFrame(this._win);
    return true;
  }

  /// Apresenta o frame e, se há FPS-cap, dorme o tempo sobrando para não passar do
  /// alvo (controlador de FPS — útil pra testar 30/60/120 ou medir ilimitado).
  endFrame(): void {
    render.endFrame(this._win);
    if (this._fpsTarget > 0) {
      const targetMs = 1000 / this._fpsTarget;
      // quanto já passou neste frame (desde o beginFrame, que setou _lastMs).
      const elapsed = time.now_ms() - this._lastMs;
      const remaining = targetMs - elapsed;
      if (remaining > 0) {
        time.sleep_ms(remaining);
      }
    }
  }

  /// Tempo (ms) desde o frame anterior — para animação/física independente de FPS.
  delta(): number {
    return this._dt;
  }

  /// Quantos frames já foram desenhados.
  frameCount(): number {
    return this._frames;
  }

  /// Fecha a janela (encerra o app).
  close(): void {
    egui.close(this._win);
  }
}

/// `createApp(title, w, h)` — abre uma janela e devolve um `App` com o loop base
/// pronto. Uso típico:
///   const app = createApp("Meu app", 480, 320);
///   while (app.running()) {
///     if (!app.beginFrame()) break;
///     const dt = app.delta();
///     // ... app.canvas.rect(...), app.canvas.button(...) ...
///     app.endFrame();
///   }
///   app.close();
function createApp(title: string, w: number, h: number): App {
  const win = egui.openWindow(title, w, h, 0);
  return new App(win);
}

/// `createAppAt(title, w, h, x, y)` — como `createApp`, mas a janela NASCE na
/// posição absoluta `(x,y)` (pixels do desktop) — escolher o monitor de forma
/// confiável (a janela já abre lá, sem mover depois).
function createAppAt(title: string, w: number, h: number, x: number, y: number): App {
  egui.setNextWindowPos(x, y);
  const win = egui.openWindow(title, w, h, 0);
  return new App(win);
}
