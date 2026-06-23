# Analise: rts-constraints

Agora tenho o contexto completo. Vou preparar uma análise cuidadosa e estruturada:

## ANÁLISE COMPLETA: ESTADO ATUAL E RESTRIÇÕES DO MOTOR HTML/CSS DO RTS

### 1) ESTADO ATUAL: ARQUITETURA ATUAL (html.rs, ctx.rs, frame.rs)

**Arquivo: `E:\rts\crates\rts-egui\src\html.rs` (173 linhas)**
- **Parser**: char-by-char manual (função `tokenize`, linhas 38-82)
  - Lê `<` até `>` coletando tags
  - Decodifica 3 entidades (`&amp;`, `&lt;`, `&gt;`)
  - Output: `Vec<Token>` (enum com `Tag { name, close }` e `Text(String)`)

- **Parsing para fila plana** (`parse_html_to_cmds`, linhas 97-158)
  - Pilha de estilos inline (`bold`, `italic`) — apenas 2 flags
  - Acumula texto em heading aberto ou emite direto como `InlineText`
  - Output: `Vec<WidgetCmd>` (sem árvore, sem DOM)
  - **Limitação estrutural**: a pilha de estilos é 1 byte por nível; não modela herança de atributos, precedência, ou ancestralidade

**Arquivo: `E:\rts\crates\rts-egui\src\ctx.rs` (175 linhas)**
- **Enum `WidgetCmd`** (linhas 82-106): 8 variantes
  - Blocos: `Heading{level,text}`, `ParagraphBegin`, `ParagraphEnd`
  - Inlines: `InlineText{text,bold,italic}`
  - Layout: `HorizontalBegin`, `HorizontalEnd`
  - Widgets: `Label`, `Button`, `Slider`
  - **Estrutura**: fila plana com pareamento por ordem (índice), não por ID ou nesting explícito

- **UiCtx** (linhas 113-135)
  - `cmds: Vec<WidgetCmd>` — fila drenada por frame
  - `button_results`, `slider_results` — resultados do frame anterior (latência 1 frame)
  - Cursores `button_cursor`, `slider_cursor` — casamento por posição

**Arquivo: `E:\rts\crates\rts-egui\src\frame.rs` (408 linhas)**
- **Drenagem recursiva** (`drenar`, linhas 215-289)
  - Função recursiva que consome a fila linearmente
  - Em `HorizontalBegin`: abre `ui.horizontal(|hui| ...)` e chama a si mesma recursivamente
  - Em `HorizontalEnd`: retorna (fecha o `ui.horizontal`)
  - Em `ParagraphBegin`: abre `ui.horizontal_wrapped(...)` e recursão similar
  - **Invariante**: `*idx` é um cursor único compartilhado por todos os níveis → ordem global preservada
  - **Conversão**: cada `WidgetCmd` vira uma chamada egui (`ui.label()`, `ui.button()`, etc.)

**Conversão final para egui**:
```
WidgetCmd::InlineText{text,bold,italic} → ui.label(RichText::new(text).strong().italics())
WidgetCmd::Heading{level,text} → ui.heading(RichText::new(text).size(28/22/18))
WidgetCmd::Button(label) → ui.button(label).clicked()
```

---

### 2) DOUTRINA: PRIMORDIAL-vs-REGISTRY (CLAUDE.md, linhas 190-315)

**Regra nuclear** (CLAUDE.md, linhas 190-202):
- **Engine (codegen) pode referenciar APENAS primitivos**: String, Object, Array, Function, Promise, Boolean, Number, Error+subclasses
- **Tudo o mais via Registry** (data-driven, não hardcoded): Map, Set, Date, Symbol, URL, RegExp, Console, Fetch, Proxy, Proxy, Reflect, backend
- **"Rust expõe primitivos, lógica em TS"** (CLAUDE.md, linhas 635-637, lib.rs linhas 2-6)

**Aplicação concreta** (CLAUDE.md, linhas 223-257):
- **Native syntax = PRIMITIVO** → codegen-direto
  - Regex `/re/` → **tem sintaxe nativa → PRIMORDIAL** (verificado no divisor "native syntax", linhas 234-235)
  - Literais array `[]`, object `{}`, string `""`, function, números
- **Sem native syntax = Registry/stdlib** → indiretamente via MethodSpec
  - `Date`, `Map`, `Set`, `JSON`, `URL` → não aparecem no codegen
  - `.ts` stdlib (`rts-shared/src/stdlib/*.ts`) para collections sem forma nativa

**Aplicação ao motor HTML/CSS**:
- HTML/CSS não têm sintaxe nativa no JS/TS → **devem viver fora do codegen**
- Se HTML fosse primordial (template literals e/ou `html` builtin no engine), violaria a doutrina
- Conclusão: **motor HTML/CSS deve ser Rust (primitivos de render) OU stdlib TS (lógica), nunca no codegen**

---

### 3) LIMITES DO ENGINE TS NOVO — CONFIRMA QUE MOTOR DEVE SER RUST?

Observações do projeto (CLAUDE.md, especialmente design doc `rts-codegen-new-design.md`):

**Restrições do novo engine**:
1. **Motor é single-path** (design doc §9): `HIR → Cranelift IR`, sem MIR (congelado no engine velho)
2. **Callbacks e closures**: suportados (§3.4 design doc), mas com restrições (mutable env-records adiados para #195)
3. **Parser character-by-character**: é **justamente o que `html.rs` faz** (linhas 38-82)
4. **Numeric subset**: o engine prioriza números comprovados (i64, f64); tipos variados (enums, classes) via PolyValue (design doc §4)

**Crítica aplicada a um motor TS**:
Se tentássemos implementar em TS um **parser/layout/paint de HTML/CSS** dentro do engine ou como stdlib TS:
- **Parser char-by-char é fine** — TS consegue (rts-parser faz isso com SWC)
- **Layout é problemático** — envolve múltiplas passes (measure, arrange, paint), estado acumulado → closures com captura de estado mutável (violaria restrição #195)
- **Callbacks especializados** (fonts, metrics, rendering backend) → engine TS não pode chamar Rust a cada medição (violaria the single-path principle; cada chamada Rust é um call extern, testamos em #437 async)
- **Backend (glyph rasterization, antialiasing)** → teria que viver em Rust; TS não consegue

**Veredito**: Um motor HTML/CSS completo **decentralizaria o layout** entre TS (estructura, estado) e Rust (medição, renderização), criando uma **"picket fence" de FFI calls a cada operação**. O custo de FFI (ligações externas) em hot-path de layout torna isso inviável. **Logo, o motor HTML/CSS deve ser RUST**, com API TS de alto nível (como egui hoje).

---

### 4) ESTRUTURA: CRATE NOVA SEPARADA OU NÃO?

**Teto de 500 linhas/arquivo** (CLAUDE.md, linhas 127-134):
- Cada arquivo sob `crates/rts-codegen-new/src/` → máx 500 linhas
- Quando ultrapassa, split em **pasta/subfolder** com `mod.rs` + sibling-files

**Aplicação a HTML/CSS**:
- `rts-egui` atual = 1287 linhas totais (arquivo único em `frame.rs` = 408)
- Motor HTML/CSS adicional (parser, layout, shape, inline-cache) → seria ~1500-3000 linhas
- **Impossível caber em rts-egui sem split**

**Opções**:
1. **Nova crate `rts-html`** (isolada)
   - Vantagem: respeita separação de responsabilidades (egui = render imediato primitivo; html = parser/layout)
   - Separação limpa entre egui (display) e HTML (parsing + layout model)
   
2. **Subfolder em rts-egui** (`rts-egui/src/html/`, `rts-egui/src/layout/`)
   - Vantagem: ambos servem a janela egui final
   - Desvantagem: rts-egui fica com múltiplas responsabilidades

**Recomendação**: **Nova crate `rts-html`** (ou `rts-dom` se incluir um modelo DOM interno)
- Rust puro, sem deps de UI framework específico
- Output: array de "display items" ou "paint records" neutrales
- Consumidor (`rts-egui`) converte display items → egui::Ui calls (ou qualquer outro backend futuro)
- Respeita a regra da doutrina: "primitivos em Rust, lógica em TS"

---

### 5) COMO O RESULTADO CHEGA AO EGUI — ARQUITETURA END-TO-END

**Cenário concreto: `egui.html(string)` em TS**

```
TS (rts-shared stdlib):
  html(str) → calls __RTS_FN_NS_EGUI_HTML(h, ptr, len)
    ↓
Rust (extern "C" em rts-egui):
  __RTS_FN_NS_EGUI_HTML(h, ptr, len) {
    html_str = from_abi(ptr, len)
    cmds = parse_html_to_cmds(html_str)  ← NEW: call rts-html::Parser
    ctx.cmds.extend(cmds)
  }
    ↓
  parse_html_to_cmds (agora em rts-html):
    Parser::new(html_str)
      .parse()  ← tokenize + AST + layout resolve
      → Vec<WidgetCmd>  ← mesma interface egui.rs hoje
    ↓
  frame.rs::drenar (sem mudança):
    consome Vec<WidgetCmd> recursivamente
    emite egui::Ui calls diretos
    → egui::Context::tessellate(...) → wgpu::RenderPass
```

**Arquitetura por camada**:

| Camada | Linguagem | Crate | Responsabilidade | Arquivo/Loc |
|--------|-----------|-------|-----------------|------------|
| **App logic** | TS | `rts-shared/stdlib/html.ts` | High-level API: `html(string)` wrapper, stylesheet API, event handlers | ~50-100 loc |
| **Parser + Layout** | Rust | **`rts-html`** (NOVA) | Tokenize HTML, build AST, resolve inline/block, compute metrics, shape inference | ~800-1200 loc (split em `parser.rs`, `layout.rs`, `shape.rs`) |
| **Display list** | Rust | `rts-html` | Output abstraction: `enum DisplayItem { Text, Block, Inline, ... }` or `Vec<WidgetCmd>` | ~200 loc |
| **Egui integration** | Rust | `rts-egui/src/html.rs` | Consume display items, convert to `Vec<WidgetCmd>`, no changes to `frame.rs` | ~100-150 loc |
| **Render (egui)** | Rust | `rts-egui/src/frame.rs` | Existing drainer + egui::Ui API calls | ~400 loc (unchanged) |
| **Window/Backend** | Rust | `rts-egui/src/app.rs`, `ctx.rs` | Existing event loop, wgpu device, surface | ~500 loc (unchanged) |

**Fronteira FFI**:
```
TS → (extern "C" string ABI) → Rust::rts_html::Parser
                                    ↓
                              Vec<WidgetCmd>
                                    ↓
                              rts_egui::drenar
                                    ↓
                              egui::Ui calls
```

**Nova crate `rts-html` — organização interna**:
```
crates/rts-html/src/
  lib.rs           — export Parser, WidgetCmd
  parser.rs        — tokenize, parse AST
  layout.rs        — measure, arrange, inline/block resolution
  shape.rs         — shape tree, hidden-class inference (opcional P2)
  ic.rs            — inline-cache for style (opcional P2)
  mod.rs           — module organization
```

---

### 6) RESUMO EXECUTIVO

| Ponto | Resposta |
|-------|----------|
| **1) Estado atual** | Parser manual (html.rs 173 loc) → fila plana `Vec<WidgetCmd>` → drenagem recursiva no egui. Sem DOM tree, sem CSS, sem layout próprio. Limitação: pilha de estilos = 2 flags (bold/italic), não modela herança/precedência. |
| **2) Doutrina** | Codegen engine NUNCA nomeia HTML/CSS (não têm sintaxe nativa). Motor deve viver em **Rust (primitivos) ou TS stdlib (lógica)**, nunca no engine. Confirmado: engine novo = single-path HIR→Cranelift, sem callbacks hot-path. |
| **3) Confirmação Rust** | Callbacks/closures em TS stdlib = picket-fence FFI cada operação layout. **Inviável em hot-path**. Logo, motor HTML/CSS deve ser **Rust puro**, com superfície TS stdlib de alto nível. |
| **4) Crate nova** | Sim. `rts-html` isolada (responsabilidade única: parse + layout). rts-egui consome via `Vec<WidgetCmd>`. Respeita teto 500 linhas. |
| **5) Arquitetura end-to-end** | TS `egui.html(str)` → Rust `__RTS_FN_NS_EGUI_HTML` → `rts_html::Parser` → `Vec<WidgetCmd>` → `drenar(ui, cmds)` → egui::Ui calls. Fronteira FFI única (string in, array out). |

---

### 7) ARQUIVOS E LINHAS RELEVANTES

| Arquivo | Linhas | Relevância |
|---------|--------|-----------|
| `E:\rts\crates\rts-egui\src\html.rs` | 1-173 | Parser manual atual; referência para arquitetura nova |
| `E:\rts\crates\rts-egui\src\ctx.rs` | 78-106 | Enum `WidgetCmd` — interface entre parser e drenagem |
| `E:\rts\crates\rts-egui\src\frame.rs` | 215-289 | Função `drenar` — conversão `WidgetCmd` → egui calls |
| `E:\rts\CLAUDE.md` | 190-315 | Doutrina PRIMORDIAL-vs-REGISTRY + "Rust expõe primitivos" |
| `E:\rts\CLAUDE.md` | 127-134 | Teto 500 linhas/arquivo + split em subfolder |
| `E:\rts\crates\rts-egui\Cargo.toml` | 1-64 | Estrutura crate atual (referência para rts-html) |
| `E:\rts\docs\specs\egui-ui-crate-design.md` | 1-160+ | Visão arquitetural: TS dirige loop, Rust primitivos |

Fim da análise.