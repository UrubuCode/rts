# Design: GUI imediata via egui — crate `rts-egui` + namespace `ui`

> **Status:** spec (pré-implementação). Decisão arquitetural travada com o time;
> código vem depois desta spec. Branch: `feat/egui-ui-crate`.

## 0. Resumo executivo

O RTS ganha uma GUI nativa **cross-platform** baseada em **egui** (immediate-mode,
Rust puro, sem deps C++). O `rts:ui` documentado como "FLTK 1.x" **nunca foi
implementado** (zero código; só refs preparatórias no linker e uma linha de doc) —
então isto é *clean slate*, não migração: não há FLTK para remover.

A doutrina do projeto manda: **o motor/Rust expõe só primitivos; a API de alto
nível vive em TS.** Esta spec aplica isso à risca:

- **Rust (`rts-egui`)** expõe os **primitivos imediatos** do egui via ABI de
  handles `u64` (`extern "C"`): abrir janela, bombear eventos, começar/terminar
  frame, emitir um widget e devolver sua resposta (clicado? valor?), apresentar o
  frame. **O Rust não conhece o conceito "Button" como objeto retido** — ele
  desenha um botão e devolve "foi clicado".
- **TS** roda o **loop de render** (`while (ui.isOpen()) { … }`) e constrói a
  biblioteca de alto nível (componentes `Window`, `Button`, layout, estado) por
  cima desses primitivos.

### Decisões travadas

| Tema | Decisão |
|---|---|
| Lib de GUI | **egui** (immediate-mode, Rust puro, cross-platform) |
| FLTK | **Descartado** (nunca existiu; atualizar docs) |
| Crate | **Nova crate `rts-egui`** (não enfiar no `rts-std`) |
| Quem dirige o loop | **TS** roda `while(ui.isOpen()){ beginFrame → widgets → endFrame → present }`; Rust bombeia winit/janela por baixo |
| Backend de render | **wgpu primário** (GPU moderna — jogos/browser futuro), **glow** como fallback de compat; escolhível pelo dev |
| Profundidade da API | **Ampla** (widgets + windows/panels), construída em TS |
| Visão de longo prazo | A crate é um **fundamento de render**, não "só widgets": habilita **jogos e até um motor de browser** no futuro. Exige **acesso à GPU/surface** além do egui (cena custom + egui como overlay) |
| Entrega | **Spec completa primeiro** (este doc), implementação depois |

## 1. Por que egui e por que crate nova

- **Cross-platform real:** Windows, macOS, Linux (e Web/Wasm) sem mudar código.
- **Rust puro:** sem toolchain C++/bindgen como FLTK exigiria; o `runtime_support.a`
  não precisa embutir objetos C++.
- **Immediate-mode casa com a doutrina:** o estado de domínio mora no *app* (no
  nosso caso, no TS), e os widgets são re-emitidos a cada frame. Isso é
  exatamente "primitivos no Rust, lógica no TS".
- **Crate isolada (`rts-egui`):** egui+winit+glow/wgpu trazem um grafo de deps
  pesado (gfx, janela). Isolar numa crate própria:
  - mantém `rts-std`/`rts-runtime` enxutos para quem não usa GUI;
  - permite **feature-gate** o backend (glow vs wgpu) sem poluir o resto;
  - respeita o teto de complexidade por crate.

## 1b. Visão de longo prazo: render foundation (jogos, browser)

Esta crate **não é "só widgets"** — é o **fundamento de render nativo do RTS**. O
objetivo declarado do time é, no futuro, construir **jogos e até um motor de
browser** sobre ela. Isso impõe requisitos que vão além de uma toolkit de UI e
**moldam as escolhas desta spec desde já** (mesmo que a primeira entrega seja só
GUI):

1. **GPU moderna obrigatória.** Jogos e layout/compositing de um browser precisam
   de Vulkan/Metal/DX12, shaders próprios, render targets, e desenho fora da UI.
   Por isso **wgpu é o backend primário** (glow/OpenGL fica como fallback de
   compatibilidade, não o caminho principal). egui aqui é **um overlay** desenhado
   por cima da cena, não o dono da tela.

2. **Acesso à surface/device, não só ao egui.** A `rts-egui` deve, desde o
   design, permitir que o TS obtenha (em fases futuras) o **device/queue/surface
   do wgpu** para render customizado: limpar a tela com uma cor, desenhar
   geometria própria, rodar um render pass, e *então* sobrepor a UI do egui. Esse
   é o padrão de engines (ex.: Bevy desenha o mundo e usa egui como editor/HUD por
   cima). A API de frame é projetada para encaixar isso:

   ```
   beginFrame(h)          // input + ctx.begin_pass
     // [futuro] sceneRenderPass(h): seu render de jogo/browser aqui
     ...widgets de UI...  // egui como overlay
   endFrame(h)            // tessela egui + compõe sobre a cena + present
   ```

3. **Controle total do loop é requisito, não preferência.** Um game loop precisa
   de `input → update → render cena → render UI → present` sob controle do código
   do usuário (TS). Daí a decisão "TS dirige o loop" não ser só ergonomia: é
   pré-condição para jogos. eframe (dono do loop) é incompatível com isso — por
   isso usamos egui + winit + wgpu **sem eframe**.

> **Escopo desta entrega vs. futuro.** A 1ª entrega expõe **GUI imediata**
> (janela + widgets + loop em TS). O **render customizado de cena** (geometria,
> shaders, texturas — o que um jogo/browser exige) é fase posterior, mas a
> arquitetura aqui (crate `rts-egui` sobre wgpu, com a janela/surface sob nosso
> controle e o frame estruturado em passes) é desenhada para **não precisar ser
> refeita** quando essa fase chegar. A crate pode inclusive ser renomeada para
> algo como `rts-gfx`/`rts-render` quando o escopo crescer além do egui — o nome
> `rts-egui` reflete só a primeira capacidade.

## 2. O obstáculo central (e como a decisão o resolve)

egui `begin_pass`/`end_pass` **só geram a malha** (tessellation). Alguém ainda
precisa **abrir a janela, bombear eventos de input (winit) e rasterizar
(glow/wgpu)** — e isso é um loop Rust que, **no macOS, obrigatoriamente roda na
main-thread** (regra do winit, não do egui). Logo, *"loop 100% em TS"* é
literalmente impossível: o bombeamento de janela é Rust.

**Como reconciliamos com "TS dirige o loop":** o TS roda o laço e dirige o
**conteúdo** de cada frame; o Rust faz o I/O de janela por baixo, de forma
síncrona, dentro das chamadas primitivas:

```
// TS — o loop é visível e está em TS:
const ui = egui.openWindow("Demo", 800, 600, "glow");
while (egui.isOpen(ui)) {
  egui.beginFrame(ui);              // Rust: bombeia eventos winit + raw_input
                                    //       + ctx.begin_pass()
  if (egui.button(ui, "Salvar")) {  // Rust: emite widget, devolve clicked
    salvar();
  }
  egui.label(ui, "Olá");
  egui.endFrame(ui);               // Rust: ctx.end_pass() -> tessellate ->
                                   //       glow/wgpu paint -> swap buffers
}
egui.close(ui);
```

Dentro de `beginFrame`/`endFrame` o Rust faz `event_loop.pump_events()`
(winit `pump_events`, modo *poll*, não-bloqueante) — não cede o loop ao winit,
deixa o TS no comando. No macOS a janela/event-loop são criados na thread que
chamou `openWindow`; **o programa RTS deve chamar `ui` a partir da main-thread**
(documentar como requisito; a maioria dos casos GUI roda no fluxo principal).

> Alternativa rejeitada nesta entrega: `ui.run(callbackHandle)` com Rust dono do
> loop chamando um fn-handle TS por frame. É mais robusto cross-thread, mas
> esconde o loop do TS (contraria a decisão) e esbarra nos perigos conhecidos de
> chamar fn_ptr TS de outra thread (memórias internas #206 callconv Tail ×
> extern "C"; #1556 race + GC SuspendThread). Como aqui o loop e os widgets rodam
> na **mesma thread** do programa TS, não há chamada cross-thread de callback.

## 3. Arquitetura de crates

```
crates/
  rts-egui/                      ← NOVA crate
    Cargo.toml                   features: ["glow"] (default), ["wgpu"]
    src/
      lib.rs                     register(e: &mut Engine) + re-exports
      abi.rs                     NamespaceMember[] do namespace `ui` (símbolos)
      ctx.rs                     UiCtx: estado por janela (winit window,
                                 egui::Context, painter glow/wgpu, raw_input,
                                 fila de shapes do frame corrente)
      window.rs                  open/close/isOpen/pumpEvents (extern "C")
      frame.rs                   beginFrame/endFrame/present (extern "C")
      widgets/                   um arquivo por família de widget:
        button.rs                button, checkbox, radio
        text.rs                  label, heading, text_edit
        value.rs                 slider, drag_value, progress_bar
        layout.rs                horizontal/vertical/grid/separator/panels
        window_widgets.rs        egui::Window, CentralPanel, SidePanel
      handle.rs                  encode/decode handle u64 <-> slot UiCtx
```

- `rts-egui` depende de `rts-engine` (ABI/Engine, registro) — **não** de
  `rts-codegen-new` (doutrina: engine não nomeia não-primordiais; `ui` resolve via
  Registry).
- `rts-runtime` re-exporta `rts-egui` (como já faz com `napi`) atrás de uma
  feature `ui` (default on no desktop; off no wasm).
- O linker (`rts-linker`) troca as refs FLTK (libs X11/Pango/etc.) pelas deps de
  janela/GL conforme o backend: **glow** → libGL/EGL + xkbcommon + x11/wayland;
  **wgpu** → Vulkan/Metal/DX12 loader. Documentar por plataforma.

## 4. Modelo de loop e threading (detalhado)

- **Uma janela = um `UiCtx`** num slab/HandleTable, chave = handle `u64`
  (gen:16 + slot:48), igual aos demais namespaces.
- `openWindow(title, w, h, backend)`:
  1. cria `winit::EventLoop` (na thread chamadora; **main-thread no macOS**) +
     `Window`;
  2. cria `egui::Context` + `egui_winit::State`;
  3. cria o painter (`egui_glow::Painter` **ou** `egui-wgpu`) conforme `backend`;
  4. guarda tudo no `UiCtx`, devolve o handle.
- `beginFrame(h)`: `pump_events` (poll, não bloqueia) → traduz eventos winit em
  `egui::RawInput` → `ctx.begin_pass(raw_input)`. Guarda o `Ui`/`Context` ativo no
  `UiCtx` para os widgets do frame.
- `widget(h, …)`: emite o widget no contexto ativo, devolve a **resposta**
  primitiva (clicado: bool; valor novo: f64/string-handle). **Sem objeto retido.**
- `endFrame(h)`: `ctx.end_pass()` → `tessellate` → painter desenha +
  `textures_delta` → `swap_buffers`/present → aplica `platform_output` (cursor,
  clipboard).
- `isOpen(h)`: false quando o usuário fechou a janela (evento `CloseRequested`).
- `close(h)`: dropa o `UiCtx`, libera o slot.

**Threading:** loop + widgets rodam **na mesma thread** do programa TS — nenhuma
chamada cross-thread de callback TS, então os riscos #206/#1556 não se aplicam.
O GC (SuspendThread) vê essa thread normalmente (já registrada como main). egui
`Context` é `Send+Sync`, mas não exploramos isso nesta entrega (tudo single-thread
do ponto de vista do TS).

## 5. Superfície ABI (primitivos `extern "C"`)

Todos os símbolos seguem a convenção `__RTS_FN_NS_UI_<NAME>`. Strings entram como
`StrPtr` (ptr+len); strings de saída voltam como handle `u64` da GC; widgets
devolvem primitivos (bool/f64/u64). **Nenhum valor polimórfico cruza a borda.**

### 5.1 Ciclo de vida da janela

| Símbolo | Assinatura (lógica) | Retorno |
|---|---|---|
| `ui.openWindow` | `(title: str, w: i32, h: i32, backend: i32)` | `handle u64` |
| `ui.isOpen` | `(h: u64)` | `bool` |
| `ui.close` | `(h: u64)` | `void` |
| `ui.beginFrame` | `(h: u64)` | `void` |
| `ui.endFrame` | `(h: u64)` | `void` |
| `ui.setTitle` | `(h: u64, title: str)` | `void` |

`backend`: `0 = glow`, `1 = wgpu` (constante exposta no `.d.ts`/builtin TS).

### 5.2 Widgets (emitem no frame ativo, devolvem resposta)

| Símbolo | Assinatura | Retorno |
|---|---|---|
| `ui.label` | `(h, text: str)` | `void` |
| `ui.heading` | `(h, text: str)` | `void` |
| `ui.button` | `(h, label: str)` | `bool` (clicado) |
| `ui.checkbox` | `(h, label: str, checked: bool)` | `bool` (novo estado) |
| `ui.radio` | `(h, label: str, selected: bool)` | `bool` |
| `ui.slider` | `(h, value: f64, min: f64, max: f64)` | `f64` (novo valor) |
| `ui.dragValue` | `(h, value: f64)` | `f64` |
| `ui.progressBar` | `(h, fraction: f64)` | `void` |
| `ui.textEdit` | `(h, text_handle: u64)` | `u64` (novo string-handle) |
| `ui.separator` | `(h)` | `void` |

### 5.3 Layout e contêineres (escopo via begin/end)

Contêineres immediate-mode são pares begin/end; o TS chama um, emite filhos,
chama o outro:

| Símbolo | Efeito |
|---|---|
| `ui.horizontalBegin` / `ui.horizontalEnd` | layout horizontal |
| `ui.verticalBegin` / `ui.verticalEnd` | layout vertical |
| `ui.gridBegin(cols)` / `ui.gridEnd` | grade |
| `ui.windowBegin(title)` / `ui.windowEnd` | `egui::Window` flutuante |
| `ui.centralPanelBegin` / `ui.centralPanelEnd` | painel central |
| `ui.sidePanelBegin(side)` / `ui.sidePanelEnd` | painel lateral |

> A natureza par begin/end é o ponto onde a API TS de alto nível brilha: o
> componente `Window` em TS faz `windowBegin()` no construtor do escopo e
> `windowEnd()` no fim, escondendo o par do dev final.

## 6. Camada de alto nível em TS (onde mora a ergonomia)

Conforme a doutrina, a biblioteca de componentes **não é Rust** — é TS sobre os
primitivos. Local: pacote builtin `builtin/ui/` (padrão dos demais builtins:
`console/`, `globals/`), exportando uma API retida/declarativa por cima do loop
imediato. Esboço de uso final:

```ts
import { App, Window, Button, Slider, Label } from "rts:ui";

const app = new App("Demo", 800, 600);     // egui.openWindow por baixo
let volume = 0.5;

app.run(() => {                            // app.run roda o while(isOpen) e
  Label("Volume");                         // chama begin/endFrame por frame
  volume = Slider(volume, 0, 1);
  if (Button("Mute")) volume = 0;
});
```

`App.run(frameFn)` em TS é literalmente:

```ts
run(frameFn) {
  while (egui.isOpen(this.h)) {
    egui.beginFrame(this.h);
    frameFn();
    egui.endFrame(this.h);
  }
  egui.close(this.h);
}
```

— o loop está em TS, como decidido; o Rust só faz I/O dentro dos primitivos.

## 7. Registro no motor (doutrina PRIMORDIAL-vs-Registry)

`ui` é **não-primordial** → resolve via Registry, **nunca** nomeado no front do
motor. Concretamente:

1. `rts-egui::register(e: &mut Engine)` declara o `NamespaceSpec` `ui` com seus
   membros (padrão `e.ns("ui").member(...).done()`).
2. Adicionar um `Register { label: "ui", run: rts_egui::register, why: … }` em
   `registry_build.rs` (`REGISTER[]`) — hoje `ui` está listado como
   "deliberately absent until a test needs them"; sai dessa lista e entra no
   array, **atrás da feature `ui`**.
3. O símbolo JIT é derivado de `SPECS` (`abi_gen.rs`) — nada de `add_fn!` manual.
4. A camada TS de alto nível é prelude/builtin, não Rust.

## 8. Backend escolhível (wgpu primário, glow fallback)

- `rts-egui` declara features `wgpu` (**default**) e `glow`, ambas compiláveis
  juntas; o `backend: i32` de `openWindow` escolhe em runtime quando as duas
  estão presentes.
- **wgpu (primário, default)**: Vulkan/Metal/DX12/GL. É o caminho para a visão de
  longo prazo (§1b): shaders próprios, render targets, cena custom + egui overlay.
  Jogos e um motor de browser **exigem** este backend. Compila mais pesado — é o
  custo de ter GPU moderna.
- **glow (fallback de compat)**: OpenGL/ES, deps leves, bom em VM/máquina antiga
  ou onde wgpu não inicializa. Suporta a GUI imediata, **mas não** o render de
  cena avançado das fases futuras — é um fallback, não o caminho principal.
- O `runtime_support.a`/linker linka as libs conforme as features ativas
  (documentar a matriz por plataforma em `rts-linker`).
- **Gancho de futuro:** o `UiCtx` guarda o `wgpu::Device`/`Queue`/`Surface` de
  forma que uma fase posterior exponha `ui.gpuDevice(h)` / `ui.beginScenePass(h)`
  para render customizado (jogos/browser), com o egui tesselado e composto por
  cima no `endFrame`. Não implementado na 1ª entrega, mas o `UiCtx` é estruturado
  para suportá-lo sem refatorar.

## 9. Versionamento

egui/eframe quebram API entre minors. **Fixar versão exata** (linha 0.34.x à época
da spec) no `Cargo.toml` da `rts-egui`. Usaremos egui + egui-winit + egui_glow /
egui-wgpu **sem eframe** (precisamos do controle do loop para o TS dirigir o
frame; eframe seria dono do loop — incompatível com a decisão).

## 10. Fases de implementação

> Cada fase roda a suíte incrementalmente; a fase 1 é o gate de viabilidade.

- **P0 — Esqueleto da crate.** `rts-egui` compila vazia, registrada atrás da
  feature `ui`; `cargo build` verde; `rts apis` lista o namespace `ui` (sem
  membros ainda).
- **P1 — PoC do loop (gate de risco).** `openWindow` + `isOpen` + `beginFrame` +
  `endFrame` + `label` + `button`, backend **wgpu** (o primário). Um `.ts` abre
  janela, roda `while(isOpen)`, mostra label + botão que conta cliques. **Valida o
  ponto crítico:** o `while`-loop TS chamando primitivos de janela não trava o GC
  nem o runtime, e o wgpu inicializa na thread chamadora. Se travar, reavaliar
  para o modelo `ui.run(callback)`.
- **P2 — Widgets básicos.** checkbox, radio, slider, dragValue, textEdit,
  separator, progressBar, heading.
- **P3 — Layout/contêineres.** horizontal/vertical/grid + Window/CentralPanel/
  SidePanel (pares begin/end).
- **P4 — Backend glow** (fallback) atrás da feature; `backend` em runtime; matriz
  de linker por plataforma.
- **P5 — Camada TS de alto nível** (`builtin/ui/`): `App`, `Window`, `Button`,
  `Slider`, `Label`, etc., com `.d.ts`. Exemplos + testes.
- **P6 — Docs.** Atualizar `CLAUDE.md` e `01-architecture.md` (trocar "FLTK 1.x"
  por "egui"), entrada no `docs/specs/INDEX.md`, remover refs FLTK do linker.
- **P7+ (futuro, fora desta entrega) — Render de cena (jogos/browser).** Expor o
  `wgpu::Device`/`Queue`/`Surface` ao TS: limpar tela, render pass custom,
  geometria/shaders/texturas, com egui composto por cima (§1b). É o que destrava
  jogos e, mais adiante, o compositing de um motor de browser. A crate pode ser
  renomeada (`rts-gfx`/`rts-render`) quando o escopo passar de UI para render
  geral. Não detalhado aqui — esta spec garante que a arquitetura P0–P6 **não
  precise ser refeita** para chegar nele.

## 11. Riscos e mitigações

| Risco | Mitigação |
|---|---|
| **Main-thread no macOS** (winit exige) | Documentar `ui` como API de main-thread; criar janela na thread chamadora. Validar no P1 em macOS se disponível. |
| **`while`-loop TS bloqueia GC/runtime** | P1 é o gate: medir tick do GC durante o loop. Se necessário, `beginFrame` faz um `gc tick` cooperativo. |
| **egui quebra API entre minors** | Versão fixa no Cargo.toml. |
| **Peso de compilação (wgpu)** | glow default; wgpu opt-in. Crate isolada não afeta builds sem GUI. |
| **String/estado de `textEdit`** | Estado do texto mora no TS (string-handle vai e volta por frame); egui não retém. |

## 12. Impacto na doutrina (checagem)

- ✅ Engine não nomeia `ui` (resolve via Registry/`registry_build.rs`).
- ✅ Rust expõe **só primitivos**; ergonomia em TS (`builtin/ui/`).
- ✅ Sem builtins de alto nível no motor; sem Symbol; sem hardcode de nome
  não-primordial no front.
- ✅ Crate isolada respeita o teto de complexidade e o particionamento de camadas.
