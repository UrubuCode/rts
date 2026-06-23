# Motor de render HTML do RTS — ROADMAP OPERACIONAL (F0-F5)

> **Este é o plano de execução vivo do motor de render HTML do RTS.** É a única
> fonte de trabalho picado. O [`rts-html-north-star.md`](rts-html-north-star.md)
> (a antiga `PLANO.md` de 5 árvores) é referência conceitual congelada e NÃO
> dita fases.
>
> Decisão tomada em 2026-06-23 após análise multi-agente (4 abordagens × 3 lentes
> adversariais — viabilidade-no-motor-TS, doutrina, custo/risco — + crítica de
> completude). Linguagem de código: Rust (inglês). Comunicação: português.

---

## 1) A estratégia em uma frase

**Evoluir o motor leve já na main (a "abordagem B": DOM retido em arena +
alocador de blocos data-driven em TS + mutação por NodeId, tudo em
`crates/rts-egui/`) IN-PLACE, por estágios usáveis. O egui é o motor de layout e
medição de texto POR PADRÃO e para sempre nos casos comuns. CSS (cor / caixa /
cascade) e eventos chegam cedo por extensões de ABI com SLOT NUMÉRICO OPACO +
cascade-em-Rust-em-lote. O paint absoluto (`allocate_painter` +
`LayoutJob`/`galley.rows`) entra como EXCEÇÃO CIRÚRGICA num único `render_*` SÓ no
ponto em que o egui comprovadamente não compõe (parágrafo inline rico com
link/hit-test). NUNCA criar `rts-html`, recriar `dom.rs`/tokenizer/ABI, nem
persistir handle de string ou NodeId entre frames.**

Por que não as 5 árvores do north-star: reescrever B numa crate destruiria 24
testes headless, a ABI provada e o fix de GPU compartilhada, em troca de pureza
arquitetural que o egui torna desnecessária. A regra "paint absoluto universal"
do north-star §3 cobra 40-60% do esforço (o Risco 1 do próprio north-star) sem
mover a ergonomia para o desenvolvedor TS.

---

## 2) Os 10 pontos de decisão, resolvidos

| # | Ponto | Decisão |
|---|---|---|
| 1 | Caminho leve oficial OU 5 árvores? | **Leve (B) é oficial e permanente.** As 5 árvores nunca nascem como pipeline global; o máximo é um mini-layout-de-parágrafo dentro de UM `render_*` (F4). |
| 2 | Layout no egui ou motor próprio? *(a contradição central)* | **egui faz o layout por default, sempre, nos 4 displays; paint absoluto é exceção de escopo (F4).** Heterogeneidade consciente: cada display escolhe seu motor. |
| 3 | Criar `rts-html` ou continuar em `rts-egui`? | **100% em `rts-egui`.** `dom.rs`/`block.rs`/`html.rs`/`style.rs` já são egui-free e testáveis headless dentro da crate. A inversão `TextMeasurer` cross-crate é cerimônia que só o paint universal justificaria. |
| 4 | Fronteira de medição de texto | **`LayoutJob`/`galley.rows` do egui, sempre.** `glyph_width` run-a-run (porta fechada pelo próprio north-star, Risco 1) NUNCA é implementado. |
| 5 | Estilo: `BlockDef` ou `ComputedStyle`+cascade? | **Evolução aditiva.** `BlockDef` permanece como UA-stylesheet de *display*; nasce um `ComputedStyle` por NodeId, calculado por cascade EM RUST (default-tag < `.classe` < `#id` < `style` inline), alimentado por slots numéricos opacos. |
| 6 | Eventos/hit-testing OU só mutação? | **Ambos.** Mutação por NodeId fica (ativo de B que o north-star nem previu). Eventos entram por **polling com contrato agnóstico ao mecanismo** (`pollEvent(h) → NodeId i64`, sentinela `-1`). Sem listener reativo (bloqueado por #195). |
| 7 | Fachada ergonômica dado os limites do motor | **Contrato cru por NodeId é a base estável; a ergonomia é uma lib `.ts` injetada via prelude no programa achatado (padrão `CONSOLE_TS`), NÃO importada de outro módulo.** Sem callback capturante; sem encadeamento sem anotação; getter de string vira `getText(node) → Handle` re-lido a cada uso. |
| 8 | Cache/incrementalidade | **Adiar até MEDIR; dono = `UiCtx`.** `ComputedStyle` num `Vec` paralelo à arena com dirty-flag grosso; galleys absolutos cacheados só no E-painter, invalidados por hash(texto+estilo+largura)+**DPI**. Sem Arc-de-árvore (Risco 6 do north-star). |
| 9 | Subset CSS/HTML e cortes | **Herdar todos os cortes permanentes do north-star** (flexbox/grid-CSS/position/z-index/transform/var()/:hover-reativo/RTL/font-family). IN incremental: cor/bg/font-size → margin/padding/border/width% → inline rico+links. |
| 10 | Reaproveitamento concreto | **Reescrever SÓ funções internas de `frame.rs`.** `dom.rs`/`block.rs`/`html.rs`/ABI/tokenizer/`present()`/`SharedGpu` ficam intactos. |

---

## 3) Os 6 invariantes duros

Estes não são notas de rodapé — são condições de aceite de qualquer PR do motor.
Surgiram da crítica de completude (lacunas que NENHUMA abordagem original cobria).

1. **Handle de string NUNCA persistido entre frames.** O loop `run()` chama
   `getText` por frame; um Handle guardado sem raiz na pilha é coletado em
   `finish_cycle()` (a cada 256 allocs, GC mark+sweep). `getText → Handle` é
   sempre re-lido, nunca cacheado no TS. A fonte da verdade do texto é a arena
   Rust (sem espelho TS — elimina dessincronia também).
2. **NodeId versionado `{idx, gen}`.** Sem geração, um NodeId reciclado após
   re-parse aplica estado a um nó vivo errado (bug de **segurança de memória**).
   Toda estrutura indexada por NodeId valida `gen`. Pré-requisito de F2/F3/F5.
3. **Sentinela `i64 = -1`, nunca `u64::MAX`.** `0xFFFF_FFFF_FFFF_FFFF` > 2^53 não
   é exato como `number` e a comparação inline erra. Todos os retornos
   NodeId-opcionais usam `-1` + regra "extrair retorno para const antes de
   comparar" (ver [[project_codegen_i64_cmp_bug]]). Padroniza `query*`,
   `pollEvent`, `createElement`.
4. **Slot numérico opaco para CSS.** O Rust NUNCA dá match em string CSS
   (`"background-color"`). `defineStyle`/`setStyle`/`setStyleBatch` recebem um
   índice; o TS mapeia nome-CSS → índice (igual a `display = 0..3` no `block.rs`).
   Critério de revisão: adicionar `box-shadow` exige só registrar slot no TS,
   nunca tocar `style.rs`. (Doutrina: o front nunca nomeia vocabulário não-nativo.)
5. **Fachada achatada via prelude `.ts`, nunca import de outro módulo.** `new` de
   classe importada de outro módulo baila no motor novo (ver
   [[project_new_engine_dispatch_limits]]); a fachada `Element`/`Document` é
   injetada no programa achatado (padrão `CONSOLE_TS`), e a API é funções
   top-level recebendo handle, nunca encadeamento `query().setText()` sem anotação.
6. **Forma EM LOTE obrigatória para estilo.** Estilizar 1 nó são 5+ props;
   cascade sobre N nós seria N×5 FFIs/frame, e o motor já é ~6× lento em workload
   array-heavy (ver [[project_array_perf_and_int32]]). `setStyleBatch(h,
   buffer_handle)` com `(nodeId, slot, val)[]` desde F2.

---

## 4) Roadmap faseado

Ordenado por **valor-por-esforço e pixel-cedo**. Cada fase entrega janela usável.
A contradição central (egui-layout × paint absoluto) é enfrentada SÓ em **F4** —
o primeiro ponto onde o egui *comprovadamente* não compõe; F0-F3 já entregam a
rede de segurança (se F4 atrasar, nada regride).

### F0 — Fundação de segurança (zero pixel novo). PRÉ-REQUISITO DE TUDO.
- **Usável:** tudo que roda hoje continua + base sã para caches/eventos.
- **Entrega:** (a) **versionar NodeId** `{idx, gen}` (invariante 2); (b) hash da
  string HTML no `UiCtx`; (c) **split de `frame.rs`** (já > 500 linhas — o gate
  `read_before_commit.sh` dispara) em `frame/render_block.rs` /
  `render_inline.rs` / `painter.rs`; (d) `style.rs` egui-free com tipos PRÓPRIOS
  (`u32 RGBA`, `Dimension{Auto,Px,Percent}`) — **nunca** `Color32`/`FontId`/`Vec2`
  (senão o argumento anti-`rts-html` cai e a separação vira mentira); (e) **3
  fixtures de prova** (`claude-egui-*`): invoke-de-fn-handle-de-Map (provar se
  baila), `new Window` via prelude achatado compila, `getText→Handle→gc-read`
  sobrevive a `finish_cycle()`.
- **Reaproveita:** tudo; só adiciona campos. **Abandona:** NodeId-sem-geração;
  `frame.rs` monolítico. **Esforço:** baixo-médio (~1 sem).
- **Gate/risco:** se a fixture de fn-handle-de-Map bailar (provável — `funcval`
  parcial, coleções perdem tipo), o modelo de eventos de F3 já nasce sem
  armazenar função.

### F1 — Estilo de texto (cor / font-size / bg) via egui. ⭐ MAIOR VALOR-POR-ESFORÇO.
- **Usável:** doc colorido, font-size arbitrário, bg por bloco — 100% via egui
  (`RichText.color/.size/.background_color`). Demo: `egui_dom_mutacao.ts`
  estilizado.
- **Entrega:** `defineStyle(sel, slot:i64, val:i64)` + `setStyle(h, node, slot,
  val)` (slots opacos, invariante 4); **conversor string→valor em `style.rs`**
  entregue JÁ aqui (pré-requisito de F1/F2/F3); aplicação só lê o
  `Vec<ComputedStyle>`.
- **Reaproveita:** `block.rs` (defaults), o `RichText` que B já emite; só
  `render_inline`/`render_block_body` consultam `ComputedStyle`. **Abandona:**
  atributo `style` ignorado; `indent` carregando tamanho de heading.
- **Esforço:** baixo-médio (~1.5 sem). **Gate/risco:** nenhum pixel absoluto.

### F2 — Box model de bloco (margin / padding / border / bg / width%) via `egui::Frame`.
- **Usável:** cards/caixas com fundo, borda, raio e espaçamento; `width%`
  resolvido **tarde** contra o content-box do pai (evita Risco 5 do north-star).
- **Entrega:** `ComputedStyle` ganha `Dimension`; `egui::Frame{inner_margin,
  outer_margin, fill, stroke, corner_radius}` + `set_max_width`. **`setStyleBatch`
  obrigatório** desde aqui (invariante 6).
- **Reaproveita:** delega layout ao egui; estende `ComputedStyle`. **Abandona:**
  nada de B. **Esforço:** médio (~2-3 sem).
- **Gate/risco:** declarar `egui::Frame ≠ box model` (sem margin-collapse, sem
  box-sizing) como limite de produto, não bug.

### F3 — Eventos por polling (clique/hover) com contrato agnóstico ao mecanismo.
- **Usável:** `<a>`/`<button>` clicáveis; loop TS faz dispatch por NodeId.
- **Entrega:** **contrato definido ANTES de implementar:** `pollEvent(h) →
  (NodeId i64 = -1 se nenhum, coord_local opcional)`. F3 usa
  `ui.interact`/`Response.clicked()` (egui faz o hit-test), mas o contrato já
  prevê o caminho painter de F4 — evita reescrever `pollEvent` depois. Handlers
  **não armazenam fn**: `pollEvent` + switch-por-NodeId no loop, estado em gcell
  module-level (contorna #195 e o invoke-de-fn-de-Map que pode bailar).
- **Reaproveita:** padrão `button_results`/cursor de `widgets.rs`;
  `id_index`/NodeId versionado de F0. **Abandona:** `onClick(()=>count++)`
  capturante (baila; exemplo-vitrine reescrito p/ gcell).
- **Esforço:** médio (~2-3 sem). **Gate/risco:** latência 1 frame (teto conhecido,
  = button/slider).

### F4 — O CORAÇÃO restrito: parágrafo inline rico + links via paint absoluto cirúrgico. AQUI A CONTRADIÇÃO CENTRAL É ENFRENTADA.
- **Por que aqui:** primeiro e único ponto onde o egui comprovadamente não compõe
  — spans mistos (negrito+link+texto) na mesma linha que quebra, com hit-test por
  run. Antes disso o egui basta; depois não há ganho. F0-F3 são a rede: se F4
  falhar, o resto não regride.
- **Pré-spike obrigatório (1-2 dias, KILL-GATE):** renderizar UM parágrafo painter
  ENTRE dois blocos egui e provar o casamento de baseline/avanço vertical via
  `ui.allocate_space(galley.size())`. Se a fronteira não casar em N dias →
  **congela em F3** (já usável) e abre issue. Converte o maior risco-tardio em
  risco-cedo.
- **Entrega:** SÓ o ramo WRAP rico monta UM `egui::text::LayoutJob` (um
  `LayoutSection` por run) → `f.layout_job(job) → Arc<Galley>` → lê `galley.rows`;
  pinta com `allocate_painter`; hit-testa link por linha-de-galley → devolve
  NodeId pelo contrato de F3. WRAP de texto puro continua no `horizontal_wrapped`.
- **Reaproveita:** tokenizer/DOM/`ComputedStyle` inteiros; o medidor do egui
  (`LayoutJob`) — não implementa `glyph_width`; `present()`/`SharedGpu` intactos.
  **Abandona:** WRAP rico via `horizontal_wrapped`+label-por-filho (compõe
  "funciona-mas-errado": baseline desalinhado).
- **Esforço:** ALTO, **2-3 semanas** (hit-test entre frames + baseline-matching
  ficam DENTRO daqui e o egui não os resolve). **Gate/risco:** MÉDIO-ALTO,
  confinado a um `render_*`.

### F5 (condicional/opcional) — Cache de galley + entidades/seletores sob demanda.
- **Usável:** parágrafos absolutos sem re-layout/frame; entidades e seletores
  compostos quando uma fixture pedir.
- **Entrega:** cache `Arc<Galley>` por NodeId no `UiCtx`, invalidado por
  hash(texto+estilo+largura)+**DPI desde o início** (esquecer DPI = texto borrado
  ao trocar de monitor). Só galleys absolutos entram.
- **Reaproveita:** thread_local `UiCtx`; hash de F0. **Abandona:** Arc-de-árvore
  do north-star. **Esforço:** baixo (~3-5 dias), condicional a medição.

**Custo total honesto:** F0-F3 ≈ 6.5-9.5 semanas (motor "rich text + caixas
estilizadas + clique" usável e demonstrável); F4 ≈ +2-3 semanas; F5 condicional.
A coexistência de dois renderizadores existe **apenas dentro do ramo WRAP de
`frame.rs`** (não no motor inteiro) e tem **kill-gate verificável** (teste que
falha se WRAP-rico ainda cair no `horizontal_wrapped` após F4).

---

## 5) Kill-gates verificáveis

Mecanismos que falham o build/teste se um invariante for violado — não confiamos
em disciplina manual:

- **F0:** `frame.rs` split (o gate `read_before_commit.sh` já dispara em > 500
  linhas — usar isso como o kill-gate do split).
- **F4:** teste que falha se o ramo WRAP-rico ainda cair no `horizontal_wrapped`
  após F4 (a coexistência tem que morrer onde devia).
- **F4 pré-spike:** o baseline-matching é provado em fixture de screenshot antes
  de comprometer 2-3 semanas.
- **Critério de teto BINÁRIO por propriedade** (não "inline-flow funciona ou
  não" — o WRAP atual já compõe "errado", não bate em muro limpo): baseline misto
  / wrap mid-run / justify são testados individualmente; cada um decide se aquele
  caso precisa do painter de F4 ou continua no egui.

---

## 6) Riscos que ainda assustam + mitigação

1. **Handle de string entre frames vaza/lê-após-free sob GC mark+sweep.**
   Mitigação: invariante 1 (`getText` re-lido, nunca persistido); fonte da
   verdade é a arena Rust. Provar no fixture `claude-egui-gettext` de F0.
2. **NodeId stale = leitura de nó morto aplicada a nó vivo.** Mitigação:
   invariante 2 (versionar em F0).
3. **Sentinela u64 cheia toma branch errado.** Mitigação: invariante 3 (`-1`).
4. **Fachada por encadeamento não compila** (dispatch sobre retorno de call sem
   anotação; `new` de classe importada baila). Mitigação: invariante 5 (funções
   top-level + prelude achatado). Validado em fixture de F0.
5. **F4 estourar prazo** (baseline + hit-test entre frames, fora do egui).
   Mitigação: pré-spike com kill-gate; F0-F3 são produto independente; congelar
   em F3 é decisão de produto formalizada, não falha.
6. **Vazamento de vocabulário CSS para o Rust** (o gate não pega — não é nome de
   classe). Mitigação: invariante 4 (slot opaco); critério de revisão.

---

## 7) A primeira fatia concreta (≤ 1 dia — valida a direção)

Provar empiricamente os 3 pontos de viabilidade incertos ANTES de comprometer o
roadmap (a estratégia inteira assume que eles compilam).

**Arquivos:**
- `crates/rts-egui/src/style.rs` (novo, egui-free): `pub struct ComputedStyle {
  color: Option<u32>, bg: Option<u32>, font_size: Option<f32> }` + `apply_slot(&mut
  self, slot: i64, val: i64)` (slots: `0=color`, `1=bg`, `2=font_size`).
- `crates/rts-egui/src/lib.rs`: 1 membro ABI `defineStyle(tag: StrPtr, slot: I64,
  val: I64) -> Void`, molde idêntico a `defineBlock`, via `e.ns("egui").member(...)`.
- `crates/rts-egui/src/frame.rs` (`render_inline`): ler `ComputedStyle` da tag e
  aplicar `RichText::new(t).color(c).size(s)`.

**ABI (convenção dura):** `StrPtr` só como arg; retorno `Void`/`I64`; nenhum
getter de string; sentinela futura = `-1`.

**Exemplo TS (`examples/claude-egui-style.ts`):**
```ts
import egui from "rts:egui";
// slots: 0=color 1=bg 2=font_size ; cores como 0xRRGGBBAA em i64
egui.defineStyle("h1", 0, 0x0088FFFF); // h1 azul
egui.defineStyle("h1", 2, 28);          // tamanho 28
egui.defineStyle("p",  0, 0xCCCCCCFF);
// ... loop run() existente desenha; valida cor+tamanho via egui, zero painter
```

**Critério de sucesso:** janela mostra `h1` azul tamanho 28 e `p` cinza, via
`RichText` — prova que (a) slot opaco funciona sem vazar vocabulário, (b)
`defineStyle` compila no padrão `defineBlock`, (c) a fachada achatada compila. Se
qualquer um bailar, ajusta-se a forma da fachada ANTES de F1.

---

## 8) Relação com os outros documentos

- **[`rts-html-north-star.md`](rts-html-north-star.md)** — a antiga `PLANO.md` de
  5 árvores, congelada como teto teórico. NÃO dita fases. Só "acorda" se o
  critério de teto binário de F4 provar que o egui não basta além do parágrafo
  rico.
- **[`README.md`](README.md)** — índice da pasta.
- **[`arquitetura.md`](arquitetura.md)** / **[`critica-adversarial.md`](critica-adversarial.md)**
  — estudo-base (pipeline canônico, crítica que cortou o escopo). Continuam
  válidos como fundamento; alimentaram as decisões acima.
