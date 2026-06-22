# Design: GUI imediata via egui — crate `rts-egui` + namespace `ui`

> **Status:** spec (pré-implementação). Decisão arquitetural travada com o time;
> código vem depois desta spec. Branch: `feat/egui-ui-crate`.
>
> **Esta versão (v2) foi reescrita após pesquisa técnica exaustiva e
> adversarialmente verificada** das APIs reais do egui/winit/wgpu (junho 2026).
> Várias afirmações da v1 foram corrigidas — ver §13 (changelog) para o registro
> honesto do que estava otimista demais.

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
- **TS dirige o loop de render** e constrói a biblioteca de alto nível
  (componentes `Window`, `Button`, layout, estado) por cima desses primitivos.

### O que a pesquisa confirmou e o que corrigiu (TL;DR)

- ✅ **"TS dirige o loop" é VIÁVEL** — winit expõe `pump_app_events` (não-bloqueante,
  retorna controle ao chamador a cada iteração). É o caso de uso oficial.
  Confirmado em **Windows/Linux**.
- ⚠️ **"Loop 100% em TS" estava mal-fundamentado** — não porque o winit não cede
  controle (cede), mas porque o *corpo* de cada passo é estado Rust não-primitivo
  que não cruza a ABI. O modelo correto é **"loop dirigido por TS sobre primitivos
  Rust"** (§1c).
- ⚠️ **"Um widget = uma chamada FFI" só vale para widgets-folha** — containers de
  layout (`horizontal`/`Window`/`Grid`/`Panel`) são **closure-only** no core do
  egui (issue emilk/egui#1004). Exigem uma **pilha de `Ui` gerida no Rust** via
  `begin_container`/`end_container`. **É o ponto de maior incerteza — exige PoC
  dedicado** (§2.2, §6).
- ⚠️ **macOS é um bloqueador SOFT, não fatal** — render fora do loop do winit
  gera artefatos de resize ("MacOS expects applications to render synchronously
  during `drawRect`"). Por isso há um **Modelo B (callback)** de fallback (§5).
- ✅ **GC não é risco para esta feature** — está inativo no motor novo; o outro dev
  refatora o GC futuramente. O risco prático é **vazamento** de handles por frame,
  mitigável (§4.3).
- ✅ **wgpu cena+overlay confirmado** — sustenta a visão jogos/browser (§1b, §8).

### Decisões travadas

| Tema | Decisão |
|---|---|
| Lib de GUI | **egui** (immediate-mode, Rust puro, cross-platform) |
| FLTK | **Descartado** (nunca existiu; atualizar docs) |
| Crate | **Nova crate `rts-egui`** (não enfiar no `rts-std`) |
| Quem dirige o loop | **Modelo A (TS dirige): `while(ui.isOpen()){ pump → beginFrame → widgets → endFrame }`** primário em **Windows/Linux**. **Modelo B (callback): `ui.runApp(cb)`** fallback **macOS/wasm** |
| Backend de render | **wgpu primário/default** (GPU moderna — jogos/browser); **glow** fallback de compat |
| Estado da janela (`UiCtx`) | **`thread_local! HashMap<u64, UiCtx>`** na thread do TS — **não** cabe no `Entry` do HandleTable nem no `tokio_ctx` (winit/wgpu são `!Send`) |
| Profundidade da API | **Ampla** (widgets-folha 1:1 + containers via pilha de `Ui`), construída em TS |
| Visão de longo prazo | A crate é um **fundamento de render**: habilita **jogos e até um motor de browser** (cena custom wgpu + egui overlay) |
| Entrega | **Spec completa primeiro** (este doc); P1 é o **gate de risco** antes da API ampla |

## 1. Por que egui e por que crate nova

- **Cross-platform real:** Windows, macOS, Linux nativos (Web/Wasm exige o Modelo
  B — §5).
- **Rust puro:** sem toolchain C++/bindgen como FLTK exigiria; o `runtime_support.a`
  não precisa embutir objetos C++.
- **Immediate-mode casa com a doutrina:** o estado de domínio mora no *app* (no
  nosso caso, no TS), e os widgets são re-emitidos a cada frame. Isso é
  exatamente "primitivos no Rust, lógica no TS".
- **Crate isolada (`rts-egui`):** egui+winit+wgpu/glow trazem um grafo de deps
  pesado (gfx, janela). Isolar numa crate própria:
  - mantém `rts-std`/`rts-runtime` enxutos para quem não usa GUI;
  - permite **feature-gate** o backend (wgpu vs glow) sem poluir o resto;
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
   compatibilidade). egui aqui é **um overlay** desenhado por cima da cena.

2. **Acesso à surface/device, não só ao egui.** A pesquisa **confirmou** que isso
   é sólido: `egui_wgpu::Renderer` foi desenhado para o modelo "você é dono do
   frame" — `update_buffers`/`update_texture` fora do pass, `render(&mut
   RenderPass, jobs, screen_desc)` **dentro de um pass que você abre no seu
   encoder**. Logo dá para: cena custom (`LoadOp::Clear` + draw da geometria) →
   egui no mesmo encoder (segundo pass `LoadOp::Load`, não apaga a cena) → um
   único `queue.submit` + `frame.present()`. Exemplo oficial: `custom3d_wgpu.rs`
   no `egui_demo_app`; `bevy_egui` confirma o padrão em produção.

   ```
   beginFrame(h)          // pump input + ctx.begin_pass
     // [futuro] ui.beginScenePass(h): seu render de jogo/browser aqui (Clear+draw)
     ...widgets de UI...  // egui como overlay (LoadOp::Load no mesmo encoder)
   endFrame(h)            // end_pass + tessellate + egui render + submit + present
   ```

   **Ressalva honesta (verificada):** a costura de lifetime do wgpu vira checagem
   de *runtime*: `egui_wgpu::Renderer::render` exige `RenderPass<'static>`, mas
   `begin_render_pass` devolve `RenderPass<'encoder>`; a ponte é
   `RenderPass::forget_lifetime()`, que **converte a validação "encoder não tocado
   durante o pass" de erro de compilação para erro de runtime**. Funciona, mas
   exige disciplina — uso indevido vira panic em runtime, não erro de build.

3. **Controle total do loop é requisito, não preferência.** Um game loop precisa
   de `input → update → render cena → render UI → present` sob controle do código
   do usuário. Daí a decisão "TS dirige o loop" não ser só ergonomia: é
   pré-condição para jogos. eframe (dono do loop) é incompatível com isso — por
   isso usamos egui + winit + wgpu **sem eframe**.

> **Escopo desta entrega vs. futuro.** A 1ª entrega expõe **GUI imediata** (janela
> + widgets + loop). O **render customizado de cena** (geometria, shaders,
> texturas) é fase posterior (P7+), mas a arquitetura aqui é desenhada para **não
> precisar ser refeita** quando essa fase chegar. A crate pode ser renomeada
> (`rts-gfx`/`rts-render`) quando o escopo crescer além do egui.
>
> **Browser via wasm está FORA do Modelo A.** winit `pump_app_events` é
> incompatível com Web (o browser não cede um loop externo de longa duração;
> eventos vêm por callback). Se "visão browser" = *rodar no browser via wasm*,
> exige o Modelo B (§5). Se "visão browser" = *construir um motor de browser
> nativo com UI estilo browser*, o Modelo A sustenta.

## 1c. O modelo de loop: o que é TS, o que é Rust (veredito verificado)

A pesquisa derrubou a fundamentação ingênua da v1 ("loop 100% em TS é impossível
porque winit não devolve controle"). **O winit devolve controle** via
`pump_app_events` (recebe `&mut self`, retorna `PumpStatus` a cada chamada — o
oposto de `run_app`, que consome o `EventLoop` e nunca retorna). A impossibilidade
real é outra, e mais simples:

**O que PODE ficar em TS:**
- A estrutura de iteração — o `while`, o teste de saída, a sequência de chamadas.
  Roda no top-level, na main thread (o JIT chama `__rtsn_main` síncrono direto, sem
  spawn — `crates/rts-codegen-new/src/front/run/module_jit.rs`).
- A condição de parada (um `bool`/`i64` devolvido por um primitivo).
- A lógica de aplicação sobre os retornos dos widgets (estado do app em variáveis TS).

**O que OBRIGATORIAMENTE fica em Rust:**
- `EventLoop`, `Window`, `wgpu::Device/Surface/Queue`, `egui::Context`, o `Ui`
  raiz — tipos Rust ricos, a maioria `!Send`/`!Sync`. **Não cabem no `enum Entry`
  do HandleTable** (fechado, primordial, em `rts-engine`) **nem no `tokio_ctx`**
  (exige `Send+Sync`). Logo o `UiCtx` vive num `thread_local! HashMap<u64, UiCtx>`
  e o TS só segura um **handle opaco u64**.
- A chamada `pump_app_events(&mut event_loop, timeout, &mut app)` exige
  referências Rust vivas que o TS não materializa. O TS chama o primitivo
  `__RTS_FN_NS_UI_PUMP`, que por dentro resolve o handle e faz a chamada.

**Conclusão:** não existe "loop sem Rust no caminho do frame". Existe **"loop
dirigido por TS sobre primitivos Rust"** — que é o modelo correto e suficiente.

## 2. Os dois pontos duros (e suas soluções)

### 2.1 macOS: bloqueador SOFT, não fatal

Render dirigido por fora do loop do winit (= `present()` por frame chamado pelo
TS) é **desencorajado pela doc do winit no macOS**, verbatim: *"If you render
outside of Winit you are likely to see window resizing artifacts since MacOS
expects applications to render synchronously during any `drawRect` callback."*
Além disso, `pump_app_events` **para o `NSApplication` entre frames** (quebra
`rfd`/file dialogs) e a doc diz *"You almost certainly shouldn't use this API."*

- **Não é panic** no caminho feliz (main = thread #0 do processo, sem `block_on`
  na main). É **degradação visual** (artefatos no resize) — e a severidade para o
  stack egui+wgpu/Metal **não está quantificada em nenhuma fonte** (lacuna
  empírica a medir num PoC macOS).
- **Windows/Linux são first-class** no Modelo A (Windows usa `PeekMessage`,
  genuinamente não-bloqueante).
- **Solução:** Modelo B (callback, §5) onde o winit possui o loop e o draw fica
  dentro do handler — portável no macOS, e o único caminho no wasm.

### 2.2 Containers de layout: closure-only no egui (o maior risco)

A pesquisa **refutou** o "um widget = uma chamada FFI" no caso geral:

- ✅ **Widgets-folha** (`button`, `slider`, `label`, `text_edit`) retornam
  `Response` **owned, sem lifetime** → `ui.button(x).clicked()` produz um `bool`
  que cruza `extern "C"` direto. Confirmado oficial + por bindings de produção
  (**Egui.NET** C# ~97% da API uma-chamada-por-método; **pyegui** Python
  one-widget-per-call).
- ⚠️ **Containers** (`Ui::horizontal`, `egui::Window`, `CentralPanel`, `Grid`)
  recebem `FnOnce(&mut Ui)` — **não têm API begin/end no core do egui** (issue
  emilk/egui#1004, reconhecida sem solução). Um botão dentro de uma `Window` exige
  um `Ui` filho que nasce dentro do closure do container.

**Solução (viável, mais trabalhosa, NÃO provada rodando — só inferida das
assinaturas):** modelar `begin_container`/`end_container` empilhando `Ui`s
manualmente do lado Rust. O `Ui` raiz é criado via `Ui::new(ctx, id, UiBuilder)`;
cada container aloca um `Ui` filho (via `UiBuilder`/`allocate_ui*`) e o empurra
numa **pilha de `Ui` no `thread_local`** indexada pelo handle do frame.
`begin_container` empurra; os widgets seguintes operam no topo; `end_container`
desempilha e finaliza — reimplementando o que `.show()` encapsula.

> **Este é o ponto que o P1 DEVE provar antes de escrever a API ampla.** Nenhuma
> fonte mostra essa pilha manual de `Ui` rodando; é a maior incerteza da spec.

## 3. Arquitetura de crates

```
crates/
  rts-egui/                      ← NOVA crate
    Cargo.toml                   features: ["wgpu"] (default), ["glow"]
    src/
      lib.rs                     register(e: &mut Engine) + re-exports
      abi.rs                     NamespaceMember[] do namespace `ui` (símbolos)
      ctx.rs                     UiCtx + thread_local HashMap<u64, UiCtx>:
                                 EventLoop, Window, egui::Context,
                                 egui_winit::State, renderer wgpu/glow,
                                 pilha de Ui do frame corrente
      app.rs                     openWindow/close/isOpen/pump (Modelo A) +
                                 runApp callback (Modelo B) (extern "C")
      frame.rs                   beginFrame/endFrame (extern "C")
      widgets/                   um arquivo por família de widget:
        button.rs                button, checkbox, radio
        text.rs                  label, heading, text_edit
        value.rs                 slider, drag_value, progress_bar
        layout.rs                begin/end de horizontal/vertical/grid/panels
        window_widgets.rs        begin/end de egui::Window
      handle.rs                  encode/decode handle u64 <-> slot UiCtx
```

> Nenhum arquivo acima pode passar de 500 linhas (regra do projeto) — `widgets/`
> já é uma pasta de submódulos coesos por esse motivo.

- `rts-egui` depende de `rts-engine` (ABI/Engine, registro) — **não** de
  `rts-codegen-new` (doutrina: engine não nomeia não-primordiais; `ui` resolve via
  Registry).
- `rts-runtime` re-exporta `rts-egui` (como já faz com `napi`) atrás de uma
  feature `ui` (default on no desktop; off no wasm).
- O linker (`rts-linker`) remove as refs FLTK (libs X11/Pango/etc.) e linka as
  deps de janela/GPU conforme o backend: **wgpu** → loader Vulkan/Metal/DX12 +
  janela (xkbcommon, x11/wayland no Linux); **glow** → libGL/EGL. Documentar a
  matriz por plataforma.

## 4. Modelo de loop e threading (detalhado)

### 4.1 Estado por janela

- **Uma janela = um `UiCtx`** num **`thread_local! HashMap<u64, UiCtx>`** (não no
  HandleTable — `UiCtx` é `!Send`). Handle `u64` = chave opaca, no estilo dos
  demais namespaces, mas o storage é local à thread do TS.
- O `UiCtx` guarda: `EventLoop`, `Window`, `egui::Context` (refcounted, cheap
  clone), `egui_winit::State`, o renderer (`egui_wgpu::Renderer` ou
  `egui_glow::Painter`), e a **pilha de `Ui`** do frame corrente.

### 4.2 Ciclo (Modelo A — TS dirige)

- `openWindow(title, w, h, backend)`: cria `EventLoop` (na thread chamadora;
  **main-thread #0 no macOS**) + `Window`; cria `egui::Context` +
  `egui_winit::State`; inicializa o renderer (wgpu init é async → resolver com
  `pollster::block_on` numa chamada síncrona); guarda no `UiCtx`; devolve o handle.
- `pump(h) -> i64`: `event_loop.pump_app_events(Some(Duration::ZERO), &mut app)`;
  o handler interno acumula `WindowEvent`s via `egui_winit::State::on_window_event`
  e trata `CloseRequested`. Retorna `0` = continue, `!=0` = sair.
- `beginFrame(h)`: `let input = state.take_egui_input(window); ctx.begin_pass(input);`
  cria o `Ui` raiz e o empurra na pilha de `Ui` do `UiCtx`.
- `widget(h, …)`: opera no `Ui` do topo da pilha; devolve a **resposta** primitiva
  (`clicked: bool`; `f64`; string-handle). **O primitivo é extraído na mesma
  chamada — a `Response` nunca é retida cruzando a FFI.**
- `beginContainer/endContainer(h, …)`: empurra/desempilha um `Ui` filho (§2.2).
- `endFrame(h)`: `let out = ctx.end_pass();` → `tessellate` →
  `state.handle_platform_output` → `renderer.update_texture/update_buffers` →
  `surface.get_current_texture` → encoder → [cena custom opcional] → pass egui
  (`LoadOp::Load` + `forget_lifetime`) → `queue.submit` → `frame.present()` (vsync)
  → `free_texture`.
- `isOpen(h)`: false após `CloseRequested`. `close(h)`: dropa o `UiCtx`, libera o slot.

### 4.3 Threading e GC

- Loop + widgets rodam **na mesma thread** do programa TS (a main #0). Nenhuma
  chamada cross-thread de callback no Modelo A — o TS chama Rust, não o inverso.
- **GC: não é preocupação desta feature.** Verificado no código: o GC está
  **inativo** no motor novo (`install_gc_hook` nunca é chamado; `is_active()`
  sempre falso; a main thread não se registra no `thread_registry`, então
  `SuspendThread` nunca a toca). O outro dev refatora o GC futuramente.
- **O risco real é vazamento, não deadlock:** sem GC, strings/arrays alocados por
  frame **vazam** durante a vida do processo. **Mitigação:** os widgets devem
  aceitar strings via `ptr+len` de **buffer reusado**, não alocar um handle de
  string novo por label por frame; reusar buffers no app; `string_free`/
  `RTS_AUTO_FREE_HANDLES=1` para o inevitável. O P1 mede o crescimento de memória
  sob N mil frames.

## 5. Modelo B (callback) — fallback macOS/wasm

Onde o Modelo A degrada (macOS: artefatos de resize) ou é impossível (wasm: sem
loop externo), o controle inverte: **o winit possui o loop** (`run_app`/
`ApplicationHandler`) e chama o TS de volta por frame.

```ts
// TS — o loop é do Rust; o frame é um callback TS:
ui.runApp({ title: "Demo", width: 800, height: 600 }, (app) => {
  ui.label(app, "Olá");
  if (ui.button(app, "Salvar")) salvar();
});
// runApp só retorna quando a janela fecha
```

Por dentro: em `RedrawRequested` o Rust invoca o `fn_ptr` TS via C-ABI **direta,
na mesma thread, sem spawn e sem `CallConv::Tail`** — o caso mais seguro de
chamada Rust→TS. As memórias internas de bugs #206 (callconv Tail × extern "C" em
thread nova) e #1556 (race async paralelo + GC) **não se aplicam**: são do motor
antigo / do caminho async paralelo, não desta invocação síncrona same-thread. O
draw acontece dentro do handler → **portável no macOS** (render síncrono no
`drawRect`), e é o único modelo no wasm.

**A fachada de widgets (`ui.label/button/slider`) é idêntica nos dois modelos** —
muda só quem dirige o loop. O dev (ou a lib TS) escolhe por plataforma/intenção.

## 6. Superfície ABI (primitivos `extern "C"`)

Convenção `__RTS_FN_NS_UI_<NAME>`. Strings entram como `StrPtr` (ptr+len);
**preferir buffer reusado** a string-handle nova por frame (§4.3). Widgets devolvem
primitivos (bool→i64 0/1, f64, u64). **Nenhum valor polimórfico cruza a borda.**

### 6.1 Ciclo de vida / loop

| Símbolo | Assinatura (lógica) | Retorno | Modelo |
|---|---|---|---|
| `ui.openWindow` | `(title, w:i32, h:i32, backend:i32)` | `handle u64` | A |
| `ui.pump` | `(h)` | `i64` (0=continue) | A |
| `ui.isOpen` | `(h)` | `bool` | A |
| `ui.beginFrame` | `(h)` | `void` | A |
| `ui.endFrame` | `(h)` | `void` | A |
| `ui.close` | `(h)` | `void` | A |
| `ui.runApp` | `(title, w, h, backend, frame_fn: handle)` | `void` | B |
| `ui.setTitle` | `(h, title)` | `void` | — |

`backend`: `0 = wgpu` (default), `1 = glow`.

### 6.2 Widgets-folha (sólido — 1:1 via FFI)

| Símbolo | Assinatura | Retorno |
|---|---|---|
| `ui.label` | `(h, text)` | `void` |
| `ui.heading` | `(h, text)` | `void` |
| `ui.button` | `(h, label)` | `bool` (clicado) |
| `ui.checkbox` | `(h, label, checked: bool)` | `bool` (novo estado) |
| `ui.radio` | `(h, label, selected: bool)` | `bool` |
| `ui.slider` | `(h, value: f64, min: f64, max: f64)` | `f64` (novo valor) |
| `ui.dragValue` | `(h, value: f64)` | `f64` |
| `ui.progressBar` | `(h, fraction: f64)` | `void` |
| `ui.textEdit` | `(h, text_handle: u64)` | `u64` (novo string-handle) |
| `ui.separator` | `(h)` | `void` |

### 6.3 Containers (via pilha de `Ui` no Rust — exige PoC, §2.2)

Pares begin/end que empurram/desempilham um `Ui` filho na pilha do `UiCtx`:

| Símbolo | Efeito |
|---|---|
| `ui.horizontalBegin` / `ui.horizontalEnd` | layout horizontal |
| `ui.verticalBegin` / `ui.verticalEnd` | layout vertical |
| `ui.gridBegin(cols)` / `ui.gridEnd` | grade |
| `ui.windowBegin(title)` / `ui.windowEnd` | `egui::Window` flutuante |
| `ui.centralPanelBegin` / `ui.centralPanelEnd` | painel central |
| `ui.sidePanelBegin(side)` / `ui.sidePanelEnd` | painel lateral |

> Os pares begin/end **não existem nativos no egui** (são closure-only); esta
> camada os sintetiza gerindo a pilha de `Ui` manualmente. É o que a API TS de
> alto nível esconde: o componente `Window` faz `windowBegin()` no início do
> escopo e `windowEnd()` no fim.

## 7. Camada de alto nível em TS (onde mora a ergonomia)

Conforme a doutrina, a biblioteca de componentes **não é Rust** — é TS sobre os
primitivos. Local: pacote builtin `builtin/ui/` (padrão dos demais builtins:
`console/`, `globals/`). Esboço de uso final (Modelo A):

```ts
import { App, Label, Slider, Button } from "rts:ui";

const app = new App("Demo", 800, 600);   // ui.openWindow por baixo
let volume = 0.5;

app.run(() => {                          // app.run roda o while + pump por baixo
  Label("Volume");
  volume = Slider(volume, 0, 1);
  if (Button("Mute")) volume = 0;
});
```

`App.run(frameFn)` em TS é literalmente o loop dirigido por TS:

```ts
run(frameFn) {
  while (ui.isOpen(this.h)) {
    if (ui.pump(this.h) !== 0) break;   // bombeia eventos do SO
    ui.beginFrame(this.h);
    frameFn();                          // o dev emite widgets aqui
    ui.endFrame(this.h);                // tessela + render + present
  }
  ui.close(this.h);
}
```

No macOS/wasm a mesma `App` usa `ui.runApp(cb)` (Modelo B) por baixo — a API
pública para o dev final é a mesma.

## 8. Backend escolhível (wgpu primário, glow fallback)

- `rts-egui` declara features `wgpu` (**default**) e `glow`, ambas compiláveis
  juntas; o `backend: i32` de `openWindow` escolhe em runtime quando ambas
  presentes.
- **wgpu (primário/default)**: Vulkan/Metal/DX12/GL nativo, WebGPU/WebGL2 no wasm.
  É o caminho para a visão de longo prazo (§1b): cena custom + egui overlay. Init
  async (`request_adapter`/`request_device`) resolvido com `pollster::block_on`.
  Compila mais pesado — custo de GPU moderna.
- **glow (fallback de compat)**: OpenGL/ES, deps leves, bom em VM/máquina antiga
  ou onde wgpu não inicializa. Suporta a GUI imediata, **mas não** o render de
  cena avançado das fases futuras.
- **Gancho de futuro (P7+):** o `UiCtx` guarda `wgpu::Device`/`Queue`/`Surface` de
  forma que uma fase posterior exponha `ui.beginScenePass(h)` para render
  customizado, com o egui composto por cima no `endFrame` (§1b). O `UiCtx` é
  estruturado para suportá-lo sem refatorar.

## 9. Versionamento (conjunto coerente, junho 2026)

egui quebra API entre minors; egui/egui-wgpu/egui-winit devem ficar **em lockstep
(mesmo número)**, e wgpu/winit são fixados por eles. **Escolher o egui primeiro,
casar o resto.**

```toml
egui       = "0.34.3"   # escolher primeiro
egui-wgpu  = "0.34.3"   # MESMO número do egui (senão diverge tipos de epaint)
egui-winit = "0.34.3"   # MESMO número do egui
wgpu       = "29.0"     # fixado por egui-wgpu 0.34
winit      = { version = "0.30", features = ["pump_events"] }  # pump_app_events
egui-glow  = "0.34.3"   # backend fallback
pollster   = "0.4"      # block_on do init async numa openWindow síncrona
```

Notas que entram no `Cargo.toml`/código:
- **winit 0.30.x:** a feature `pump_events` deve estar habilitada explicitamente;
  `pump_app_events` é o método (o antigo `pump_events`-closure está deprecado). No
  modelo `ApplicationHandler` (winit 0.30) o handler interno acumula os eventos.
- **egui 0.34:** `begin_pass`/`end_pass` permanecem; `Context::run` virou
  `run_ui`, e hooks `on_begin_pass`/`on_end_pass` viraram trait `Plugin` em 0.33 —
  irrelevante para o begin/end manual, mas revalidar nomes ao subir versão. (Se
  preferir a estabilidade do `run` não-deprecado, 0.31.1/0.32.x são opção, com o
  lockstep wgpu/winit correspondente — checar.)
- **wgpu 29:** `request_device` devolve `(Device, Queue)` num future;
  `RenderPassColorAttachment` tem `depth_slice` (wgpu 26+); `render()` exige
  `RenderPass<'static>` via `forget_lifetime()`.
- **Sem eframe** (precisamos do controle do loop; eframe seria o dono dele).

## 10. Fases de implementação

> Cada fase roda a suíte incrementalmente. **P1 é o gate de viabilidade** — não
> escrever a API ampla antes de P1 passar.

- **P0 — Esqueleto da crate.** `rts-egui` compila vazia, registrada atrás da
  feature `ui`; `cargo build` verde; `rts apis` lista o namespace `ui` (sem
  membros). Linker para de referenciar FLTK.
- **P1 — PoC do loop + containers (GATE DE RISCO).** Backend **wgpu**, Modelo A,
  Windows. Provar, nesta ordem:
  1. `openWindow` na main thread com tokio já inicializado **sem panic**;
  2. `pump(ZERO)` + `beginFrame`/`endFrame` + `present()` rodando num `while` TS;
  3. **a pilha de `Ui` manual para containers** (§2.2): abrir uma `Window`/
     `horizontal`, emitir 3+ widgets aninhados, conferir layout visual correto —
     **o item mais incerto, sem precedente em fonte**;
  4. cena custom (`Clear`) + egui overlay (`LoadOp::Load` + `forget_lifetime`) no
     mesmo encoder, sem erro de runtime;
  5. N mil frames medindo crescimento de memória (vazamento §4.3).
  Se (3) falhar, reavaliar a API de containers (ex.: expor só layouts pré-montados).
  Se o resize no macOS for severo (medir se houver hardware), tornar o Modelo B
  obrigatório no macOS.
- **P2 — Widgets-folha.** checkbox, radio, slider, dragValue, textEdit, separator,
  progressBar, heading (todos 1:1, caminho sólido).
- **P3 — Containers.** horizontal/vertical/grid + Window/CentralPanel/SidePanel
  (depende de P1.3 ter validado a pilha de `Ui`).
- **P4 — Modelo B (callback) + backend glow.** `runApp(cb)` para macOS/wasm;
  `egui_glow` atrás da feature; matriz de linker por plataforma.
- **P5 — Camada TS de alto nível** (`builtin/ui/`): `App`, `Window`, `Button`,
  `Slider`, `Label`, etc., com `.d.ts`. A `App` escolhe Modelo A/B por plataforma.
  Exemplos + testes.
- **P6 — Docs.** Atualizar `CLAUDE.md` e `01-architecture.md` (trocar "FLTK 1.x"
  por "egui"), entrada no `docs/specs/INDEX.md` (já feita), remover refs FLTK do
  linker.
- **P7+ (futuro, fora desta entrega) — Render de cena (jogos/browser).** Expor
  `wgpu::Device`/`Queue`/`Surface` + `beginScenePass`: limpar tela, render pass
  custom, geometria/shaders/texturas, egui composto por cima (§1b). Possível
  rename da crate (`rts-gfx`/`rts-render`).

## 11. Riscos e mitigações (atualizado pelo veredito)

| Risco | Severidade | Mitigação |
|---|---|---|
| **Containers closure-only** (pilha de `Ui` manual, sem precedente em fonte) | **Alta — maior incerteza** | **P1.3 é o gate**; se inviável, expor layouts pré-montados em vez de begin/end arbitrário |
| **macOS: artefatos de resize** (render fora do `drawRect`) | Média (soft, não fatal) | Modelo B (callback) no macOS; medir severidade real em PoC macOS |
| **wasm incompatível com Modelo A** | Média | Modelo B é o único caminho no wasm; documentar |
| **`forget_lifetime` vira erro de runtime** | Média | Disciplina: não tocar o encoder durante o pass; cobrir em teste |
| **Lockstep de versões egui/wgpu** | Baixa | Versões fixas (§9); CI compila a crate |
| **Vazamento de handles por frame** (GC inativo) | Média | Buffers reusados (`ptr+len`), não string-handle nova por frame; P1.5 mede |
| **Peso de compilação (wgpu)** | Baixa | Crate isolada atrás da feature `ui`; não afeta builds sem GUI |
| **GC futuro reativado + render thread registrada** | Baixa (futuro) | Manter a render thread **fora** do `thread_registry`; safepoints cooperativos — **responsabilidade do refactor de GC do outro dev** |

## 12. Impacto na doutrina (checagem)

- ✅ Engine não nomeia `ui` (resolve via Registry/`registry_build.rs` — hoje `ui`
  está na lista "deliberately absent"; sai dela e entra no `REGISTER[]` atrás da
  feature `ui`).
- ✅ Rust expõe **só primitivos**; ergonomia em TS (`builtin/ui/`).
- ✅ Sem builtins de alto nível no motor; sem Symbol; sem hardcode de nome
  não-primordial no front.
- ✅ Crate isolada respeita o teto de complexidade e o particionamento de camadas;
  nenhum arquivo > 500 linhas.
- ✅ Símbolo JIT derivado de `SPECS` (`abi_gen.rs`), sem `add_fn!` manual.

## 13. Changelog v1 → v2 (correções após pesquisa verificada)

Registro honesto do que a v1 afirmava otimista demais (ver veredito adversarial):

1. **"glow default" → "wgpu default".** O ecossistema usa wgpu como render
   primário; glow é alternativa. (Também: wgpu largou o `0.` e está em 29.0.x, não
   "0.20/0.22".)
2. **"loop 100% em TS é impossível porque winit não cede controle" → corrigido.**
   winit **cede** controle (`pump_app_events`). A impossibilidade real é o estado
   Rust não-primitivo que não cruza a ABI. Modelo correto: "loop dirigido por TS
   sobre primitivos Rust" (§1c).
3. **"um widget = uma chamada FFI" → só widgets-folha.** Containers são
   closure-only (egui#1004); exigem pilha de `Ui` manual no Rust (§2.2) — o maior
   risco, a provar no P1.
4. **"funciona igual em toda plataforma" → corrigido.** macOS degrada (resize),
   wasm exige Modelo B. Windows/Linux first-class no Modelo A; Modelo B (callback)
   adicionado como fallback (§5).
5. **"GC gerenciado/seguro" → corrigido.** GC está inativo no motor novo; sem
   deadlock, mas há **vazamento** de handles por frame (§4.3). GC é
   responsabilidade do refactor futuro do outro dev, não desta feature.
6. **`UiCtx` no HandleTable → corrigido.** Não cabe no `Entry` (fechado,
   primordial) nem no `tokio_ctx` (`Send+Sync`; winit/wgpu são `!Send`). Vai num
   `thread_local! HashMap<u64, UiCtx>` (§4.1).
