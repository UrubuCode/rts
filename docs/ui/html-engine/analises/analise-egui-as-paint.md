# Analise: egui-as-paint

I now have all the API confirmations needed. Let me verify two specific 0.34 details: the `text::layout` free function and whether `Painter::text` returns the bounding `Rect` (for measuring after the fact) — I already have these. Let me confirm the `CornerRadius` type rename and `StrokeKind` (new in recent egui) to be precise about the 0.34 signatures, which I already captured. I have enough to write the report.

---

# egui 0.34 como backend de PAINT para um motor HTML/CSS próprio

Resposta curta: **sim, é sólido e é o uso recomendado.** O `Painter` do egui é uma API de desenho imediato em **coordenadas absolutas de tela (logical points)** que você pode usar totalmente desacoplada do sistema de layout do egui. Você roda *seu* box model, produz display items em `(x,y,w,h)`, e os despeja num `Painter` com `rect_filled`/`galley`/`line`/`text`. O egui continua entregando exatamente as três peças onde ele é forte e você não quer reimplementar: **medição de texto + line-breaking (Galley/Fonts)**, **atlas de fonte/rasterização**, e **scroll com viewport virtual**. Detalhes e APIs confirmadas abaixo.

## 1) Pintar em coordenadas ABSOLUTAS (egui não impõe layout aqui)

O ponto central: o `Painter` **não faz layout**. Todo método recebe `Pos2`/`Rect` em coordenadas de tela (pontos lógicos), absolutas. Você obtém um `Painter` e pinta onde quiser dentro do `clip_rect`.

Como obter o `Painter`:
- **`Ui::allocate_painter(desired_size: Vec2, sense: Sense) -> (Response, Painter)`** — reserva uma região e devolve `(Response, Painter)`. O `clip_rect` do Painter é a interseção do retângulo alocado com o clip do `Ui`; `response.rect` te dá o retângulo em coordenadas de tela onde sua origem `(0,0)` do box model deve ser ancorada (some `response.rect.min` aos seus offsets). Este é o caminho canônico para "me dá uma tela, eu desenho".
- **`Ui::painter() -> &Painter`** — painter da região atual do `Ui`.
- **`Context::layer_painter(layer_id: LayerId) -> Painter`** — painter de tela inteira numa layer; bom para overlays/camadas próprias (ex.: z-index).
- **`Context::debug_painter() -> Painter`** — por cima de tudo (debug/overlay de inspeção).
- **`Painter::new(ctx, layer_id, clip_rect)`** — construção direta.

APIs de desenho confirmadas (assinaturas 0.34):
- `rect_filled(rect: Rect, corner_radius: impl Into<CornerRadius>, fill_color: impl Into<Color32>) -> ShapeIdx` — note o rename **`Rounding` → `CornerRadius`** no egui recente; passe `CornerRadius::ZERO` (ou `0.0`) para cantos retos do box model. Backgrounds e borders de bloco saem daqui.
- `rect_stroke(rect, corner_radius, stroke: impl Into<Stroke>, stroke_kind: StrokeKind) -> ShapeIdx` — borders. O parâmetro **`StrokeKind`** (`Inside`/`Middle`/`Outside`) é novo e importa para CSS: `box-sizing`/alinhamento da borda no pixel exato do `Rect` casa com `StrokeKind::Inside`.
- `text(pos: Pos2, anchor: Align2, text: impl ToString, font_id: FontId, text_color: Color32) -> Rect` — atalho que faz layout interno e **retorna o `Rect` pintado** (útil só para medir depois do fato; para seu line-breaking, use Galley, ver §3).
- `galley(pos: Pos2, galley: Arc<Galley>, fallback_color: Color32)` — pinta texto **já medido/quebrado** por você. É o método que você vai usar de fato para texto no box model.
- `line_segment([Pos2; 2], stroke)`, `line(Vec<Pos2>, stroke: impl Into<PathStroke>)`, `circle_filled(center: Pos2, radius, fill)`, `image(texture_id: TextureId, rect: Rect, uv: Rect, tint: Color32)` — para `border`, list markers, `<img>`, etc.

Clipping (essencial p/ `overflow:hidden` e clip por bloco):
- `painter.with_clip_rect(rect: Rect) -> Painter` (filho com interseção do clip), `set_clip_rect(&mut self, Rect)`, `clip_rect() -> Rect`.

**Conclusão (1):** confirmado. Coordenadas absolutas, sem transformação. `allocate_painter` te dá a "superfície" e a origem; o resto é seu.

## 2) Pipeline "display items absolutos → Painter": viável?

**Sim, e é exatamente o design idiomático para esse caso.** Seu motor emite uma display list (`RectItem{xywh, fill, corner, border}`, `TextItem{x, y, galley, color}`, `ImageItem{rect, tex}`, …); um walker traduz cada item para uma chamada do `Painter` somando a origem (`response.rect.min`) e respeitando o `clip_rect` do bloco. Isso espelha a separação real de um browser (layout tree → display list → paint), com o egui no papel de "paint/raster backend".

Pontos de atenção que confirmam a solidez:
- **Ordem de pintura = z-order.** O `Painter` desenha na ordem das chamadas (back-to-front). Seu stacking context/`z-index` vira a ordem de iteração da display list (ou `layer_painter` por camada).
- **Sem caching automático entre frames** no modo imediato: egui re-pinta tudo a cada frame. Para páginas grandes isso é O(display items); a mitigação é o viewport virtual do ScrollArea (§5) — só gere/pinte items visíveis.
- **`PaintCallback`** existe se você quiser injetar GPU custom, mas para HTML/CSS o `Painter` cobre tudo (rects, texto, imagens, linhas, clip).

## 3) MEDIR texto antes de pintar (para o SEU line-breaking)

Aqui o egui é diretamente útil e você **não** deve reimplementar shaping/atlas. O acesso é via `Context::fonts`:

```rust
pub fn fonts<R>(&self, reader: impl FnOnce(&FontsView<'_>) -> R) -> R
```

Dentro do closure você tem `FontsView` (0.34 moveu os métodos de layout de `Fonts` para `FontsView`):
- `layout_no_wrap(text: String, font_id: FontId, color: Color32) -> Arc<Galley>` — mede sem quebrar; o `Galley` resultante te dá a **largura** da run.
- `layout(text: String, font_id: FontId, color: Color32, wrap_width: f32) -> Arc<Galley>` — se quiser que o egui quebre na sua `wrap_width` (quebra em `wrap_width` e em `\n`). Memoizado.
- `layout_job(job: LayoutJob) -> Arc<Galley>` — caminho rico (ver §4).
- `glyph_width(font_id: &FontId, c: char) -> f32` e `row_height(font_id: &FontId) -> f32` — medição granular para um **algoritmo de quebra de linha próprio** (você itera palavras/glifos, soma larguras e decide os breaks do *seu* layout).

Do `Galley` você lê dimensões:
- `galley.size() -> Vec2` (largura/altura em pontos), `galley.rect: Rect` (com `rect.top()==0.0`; o alinhamento horizontal depende de `LayoutJob::halign`), `galley.mesh_bounds`, `galley.rows: Vec<PlacedRow>`, e `intrinsic_size()` para dimensão independente de alinhamento.
- Para hit-testing (seleção/caret no seu motor): `cursor_from_pos(Vec2)` / `pos_from_cursor(CCursor)`.

**Padrão recomendado:** ou (a) você quebra linhas você mesmo com `glyph_width`/`row_height` e gera um `Galley` por run com `layout_no_wrap`, ou (b) delega a quebra de uma run a `layout(..., wrap_width)` quando a largura disponível já é conhecida pelo seu box model. Em ambos, o `Galley` final vai pro `Painter::galley(pos, galley, color)` na posição absoluta que **você** calculou. Importante: `Galley` precisa ser recriado se `pixels_per_point` muda ou o atlas enche.

## 4) `LayoutJob`/`RichText` vs `Painter::galley` — trade-off de controle de x,y

- **`RichText`** é um helper de *widget* (`ui.label(RichText...)`) — ele aciona o layout **e o posicionamento do egui**. Para um motor que controla x,y, **evite** `RichText`: você perde o controle de posição. Sirva-se dele só em chrome/UI auxiliar, não no conteúdo da página.
- **`LayoutJob` + `FontsView::layout_job` + `Painter::galley`** é o caminho certo. `LayoutJob` te dá **formatação por seção** (cada `LayoutSection` com seu `TextFormat`: `font_id`, `color`, `background`, `italics`, `underline`, `valign`) — ou seja, spans inline com fontes/cores/decoração diferentes numa mesma run, que é exatamente o que CSS inline exige (`<b>`, `<span style>`, `text-decoration`). Campos relevantes: `text`, `sections`, `wrap: TextWrapping{ max_width, max_rows, break_anywhere, … }`, `halign`, `justify`, `break_on_newline`, `first_row_min_height` (útil para continuar uma linha iniciada por conteúdo anterior — casa com fluxo inline).

Trade-off resumido: **`LayoutJob` = controle de formatação/medição; `Painter::galley` = controle de posição.** Você usa os dois juntos: monta o `LayoutJob` da run inline → `layout_job` mede e quebra → você posiciona o `Galley` em coordenada absoluta via `Painter::galley`. `RichText` só faz sentido fora do conteúdo paginado.

## 5) SCROLL com Painter absoluto — funciona, e é onde o egui ainda paga muito

Funciona, com a ressalva de que você usa **`show_viewport`**, não `show()`:

```rust
pub fn show_viewport<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui, Rect) -> R) -> ScrollAreaOutput<R>
```

- O closure recebe **`viewport: Rect` em coordenadas relativas ao conteúdo** (`viewport.min == ZERO` ⇒ topo). Isso te dá a janela visível para **virtual scrolling**: pinte só os display items que intersectam `viewport` (crucial para páginas grandes no modo imediato).
- **Tradução content→screen:** `screen_pos = content_pos + (ui.min_rect().min - viewport.min)`. O vetor `ui.min_rect().min - viewport.min` é o offset de scroll. Some isso a cada `(x,y)` da sua display list antes de chamar o `Painter`.
- Você deve **alocar o tamanho total do conteúdo** (ex.: `ui.allocate_space(Vec2::new(content_w, content_h))` / `set_min_size`) para o egui dimensionar a barra e o range de scroll corretamente.
- Configs: `ScrollArea::vertical()`, `::both()`, `auto_shrink([false,false])` (normalmente você quer travar para o conteúdo ditar tamanho). Saída em `ScrollAreaOutput` com `state.offset`.

**Conclusão (5):** ScrollArea é totalmente compatível com pintura absoluta — e mais: o `viewport` te entrega culling de graça, o que é exatamente o que você quer num motor de página grande sem cache de frame.

## Onde egui AJUDA vs ATRAPALHA

**Ajuda (mantenha):**
- **Medição/shaping de texto + line-break opcional** (`FontsView::layout*`, `glyph_width`, `row_height`, `Galley`) — não reimplemente.
- **Atlas de fonte + rasterização + upload de textura** — totalmente gerido pelo egui/epaint; `Painter::galley` consome o `Galley` que referencia o atlas.
- **Scroll com viewport virtual** (`show_viewport`) — culling + barra prontos.
- **Clipping por bloco** (`with_clip_rect`) para `overflow`.
- **Pipeline GPU/janela/eventos** (winit + wgpu/glow) via eframe — você ganha a superfície e o loop de input sem custo.

**Atrapalha / a evitar (porque é layout, não paint):**
- **Não use widgets/containers de layout** (`Ui::horizontal/vertical`, `Grid`, `Window`, `Frame`, `RichText` no conteúdo) — eles posicionam por você e brigam com seu box model. Use só `allocate_painter`/`layer_painter` + `ScrollArea::show_viewport`.
- **Modo imediato re-pinta tudo todo frame, sem retained/cache** — para páginas grandes, isto exige seu próprio descarte por `viewport` e, idealmente, manter a display list entre frames mudando só o que mudou (egui não diffa shapes por você).
- **`pixels_per_point`/DPI:** seus `(x,y)` são pontos lógicos; recrie `Galley`s quando `ctx.pixels_per_point()` mudar (atlas invalida).
- **Z-order é ordem de chamada** — você gerencia stacking contexts; egui não tem noção de z-index do CSS.

**Veredito:** usar egui 0.34 como **painter de baixo nível para display items absolutos**, com seu próprio box model, é uma arquitetura sólida e idiomática. As três alavancas que justificam o egui (medição de texto, atlas de fonte, scroll com viewport virtual) são exatamente as que você não quer reescrever; o resto (layout) você ignora deliberadamente via `allocate_painter` + `Painter` + `ScrollArea::show_viewport`.

## Sources

- [egui 0.34 `Painter`](https://docs.rs/egui/0.34.0/egui/struct.Painter.html) — `rect_filled`, `rect_stroke` (`CornerRadius`, `StrokeKind`), `text`, `galley`, `line`, `line_segment`, `circle_filled`, `image`, `with_clip_rect`/`set_clip_rect`
- [egui 0.34 `Ui`](https://docs.rs/egui/0.34.0/egui/struct.Ui.html) — `allocate_painter`, `painter`, `allocate_space`
- [egui 0.34 `Context`](https://docs.rs/egui/0.34.0/egui/struct.Context.html) — `fonts(|FontsView| …)`, `layer_painter`, `debug_painter`, `pixels_per_point`
- [epaint 0.34 `FontsView`](https://docs.rs/epaint/0.34.0/epaint/text/struct.FontsView.html) — `layout`, `layout_no_wrap`, `layout_job`, `glyph_width`, `row_height`
- [epaint 0.34 `Galley`](https://docs.rs/epaint/0.34.0/epaint/text/struct.Galley.html) — `size()`, `rect`, `rows`, `cursor_from_pos`/`pos_from_cursor`
- [egui 0.34 `LayoutJob`](https://docs.rs/egui/0.34.0/egui/text/struct.LayoutJob.html) — `sections`/`LayoutSection`/`TextFormat`, `wrap`, `halign`, `justify`, `break_on_newline`
- [egui `ScrollArea` (`containers::scroll_area`)](https://docs.rs/egui/latest/egui/containers/scroll_area/struct.ScrollArea.html) — `show_viewport(ui, |ui, viewport: Rect|)`, `vertical`/`both`
- [emilk/egui discussion #2902 — usando `ScrollArea::show_viewport`](https://github.com/emilk/egui/discussions/2902) — semântica do `viewport` (relativo ao conteúdo, `min==ZERO` = topo) e tradução content→screen

Arquivos do repo relevantes se for prototipar o backend de paint: o namespace de UI atual usa FLTK (`crates/rts-runtime/src/namespaces/ui/`), referenciado em `E:\rts\CLAUDE.md`; um backend egui entraria como namespace paralelo seguindo o mesmo padrão `mod.rs`/`abi.rs`/`<group>.rs`.