# APP_FEATURES.MD
> Documento vivo de estado atual, plano de evolução e benchmarks do RTS.
> Atualizar a cada feature concluída.
>
> **Ultima revisao: 2026-04-29**

---

## Estado Atual do Projeto

### Comandos CLI

| Comando | Descrição |
|---|---|
| `rts run <file.ts>` | Compila e executa via JIT in-memory (sem disco, sem linker) |
| `rts compile <file.ts> [out]` | Compila AOT → `.exe` nativo via Cranelift + linker |
| `rts ir <file.ts>` | Dump do IR Cranelift para stderr, sem executar |
| `rts i [pkg@ver ...]` | Instala pacotes npm de `package.json` ou argumentos |
| `rts init [nome]` | Scaffolda novo projeto com `src/main.ts`, `package.json`, `tsconfig.json`, `rts-types/` |
| `rts test [path]` | Descobre e executa `*.test.ts` / `*.spec.ts` via JIT |
| `rts emit-types` | Gera `rts-types/rts.d.ts` a partir de `abi::SPECS` |
| `rts apis` | Lista todos os namespaces e membros registrados em `abi::SPECS` |
| `rts clean` | Remove caches em `node_modules/.rts/` |
| `rts eval / -e` | Executa snippet TS inline via JIT (sem arquivo); lê stdin se não-tty |

### Pipeline de execução

```
Source TS
  → Parser (SWC)
  → type_system
  → codegen/lower/ (Cranelift IR)
       ├── JIT:  JITModule → memória executável → chama __RTS_MAIN
       └── AOT:  ObjectModule → .o → linker → .exe
```

Não existem camadas HIR/MIR. O codegen consome o AST diretamente em `src/codegen/lower/`.

### Artefatos em disco

| Caminho | Conteúdo |
|---|---|
| `~/.rts/artifacts.a` | Runtime support (todos os namespaces compilados). Atualizado automaticamente quando o binário `rts` muda (comparação SHA-256). |
| `~/.rts/register/npm/<name>/<version>/` | Cache global de pacotes npm instalados |
| `~/.rts/globals/` | Symlinks para pacotes instalados globalmente (`rts x <nome>`) |
| `node_modules/.rts/obj/<hash>/` | Objetos compilados por projeto (cache incremental AOT) |
| `node_modules/.rts/modules/` | Módulos remotos e npm cacheados por projeto |
| `rts-types/rts.d.ts` | Tipos gerados por `rts emit-types` — na raiz do projeto, gitignored |
| `rts.lock` | Lockfile de dependências (gerado por `rts i`) |

### Namespaces ativos (35)

`io`, `fs`, `gc`, `math`, `num`, `bigfloat`, `time`, `env`, `path`, `buffer`, `string`,
`process`, `os`, `collections`, `hash`, `fmt`, `crypto`, `net`, `tls`, `thread`,
`atomic`, `sync`, `parallel`, `mem`, `hint`, `ptr`, `ffi`, `regex`, `runtime`,
`test`, `trace`, `ui`, `alloc`, `json`, `date`

### ABI — contrato único (`src/abi/`)

- Cada função de namespace é um `#[unsafe(no_mangle)] pub extern "C"` tipado
- Símbolo: `__RTS_FN_NS_<NS>_<NAME>` (ex: `__RTS_FN_NS_IO_PRINT`)
- Codegen emite `call <symbol>` direto via Cranelift — sem dispatch, sem boxing, sem JsValue
- `AbiType`: `Void | Bool | I32 | I64 | U64 | F64 | StrPtr | Handle`
- `StrPtr` → dois slots Cranelift: `(i64 ptr, i64 len)`
- Intrinsics inline: `sqrt`, `abs_f64/i64`, `min/max_f64/i64`, `random_f64`

### Pacotes — `rts i`

- Instala de `package.json` ou argumentos (`rts i axios@0.27.2`, `rts i @org/pkg`)
- Resolve semver ranges (`^`, `~`, `>=`)
- Baixa tarball npm → extrai em `~/.rts/register/npm/<name>/<version>/`
- Copia ou symlink (`RTS_SYMBOL_NODE_MODULES=1`) para `node_modules/<name>/`
- Cria wrappers em `node_modules/.bin/`
- Salva `rts.lock` (JSON, `lockfileVersion: 1`)
- Import resolver resolve `node_modules/` automaticamente (sem declarar em `package.json`)

### `.env` — carregamento automático

`rts run` lê `.env` do diretório do arquivo de entrada antes de executar.
Variáveis já definidas no ambiente não são sobrescritas.

### Silent parallelism

Três passes de reescrita automática em `src/codegen/lower/`:

| Pass | Padrão detectado | Reescrito para |
|---|---|---|
| `array_methods_pass` | `arr.map(fn)`, `arr.forEach(fn)`, `arr.reduce(fn, init)` | `parallel.map/for_each/reduce` |
| `reduce_pass` | Acumulador com loop `for...of` (ops `+` e `*`) | `parallel.reduce` |
| `purity_pass` | `for...of` com corpo 100% de fns `pure: true` | `parallel.for_each` |

### Capacidades de linguagem (codegen)

- Literais: objetos `{k:v}`, arrays `[1,2,3]`, template literals
- Classes: constructor, métodos, `this`, `extends`, `super(args)`, `super.method()`, getters/setters, static
- Operator overload: `a + b` → `a.add(b)` quando classe define o método
- `for...of` em arrays; `for...in`; `while`; `do...while`
- `try/catch/finally` fase 1 (slot thread-local, sem unwind real)
- String equality: `s1 == s2` → `gc.string_eq`
- Tail call optimization: `return f(x)` em posição de cauda → `return_call`
- Function pointers: `Expr::Ident` de user fn → `func_addr` → `call_indirect`
- Jump table switch (literais inteiros)
- Imm forms: `iadd_imm`, `band_imm`, `ishl_imm`
- Destructuring, spread, rest args, default args
- Comma operator, logical assignments (`||=`, `&&=`, `??=`)
- Ternary, optional chaining, nullish coalescing
- Bitwise ops, comparações, short-circuit

### Benchmarks (medianas, Windows 11 AOT+JIT vs Bun/Node)

| Bench | RTS JIT | RTS AOT | Bun | Node |
|---|---|---|---|---|
| Monte Carlo 10M | 119 ms | 156 ms | 173 ms | 281 ms |
| Machin bigfloat 30 dígitos | 47 ms | 48 ms | 109 ms | 108 ms |

---

## Plano de Evolução

### P1 — Workspace Cargo (rts-compiler / rts-runtime)

**Motivação:** FLTK, rayon, rustls, regex compilam junto com o compilador.
Mudança em `io.rs` recompila tudo. `jit.rs` tem 1500 linhas com tabela manual de símbolos.

**O que muda:**

```
rts/                     (workspace)
  rts-compiler/          parser, codegen, CLI, pipeline — zero deps de UI
  rts-runtime/           namespaces: gc, io, fs, math, ... (sem UI)
  rts-runtime-ui/        namespace ui, backend-agnostic
  rts-ui-fltk/           FLTK impl de UiBackend
  rts-ui-webview/        wry impl (Android/iOS)
```

- Cargo compila `rts-runtime` como `staticlib` — elimina `build.rs` com `rustc` manual
- `jit.rs`: substituir tabela `runtime_symbol_table()` por resolução `GetProcAddress`/`dlsym` iterando `SPECS` — reduz ~450 linhas

**Ganho esperado:** incremental build 40-60% mais rápido. Qualquer namespace novo em `SPECS` funciona no JIT automaticamente.

---

### P2 — UI cross-platform via `UiBackend` trait

**Motivação:** FLTK não roda em Android/iOS. `UiEntry` enum colado em FLTK — trocar backend exige reescrever tudo.

**Arquitetura:**

```rust
pub trait UiBackend: Send + 'static {
    fn window_new(&self, w: i32, h: i32, title: &str) -> UiHandle;
    fn button_new(&self, x: i32, y: i32, w: i32, h: i32, label: &str) -> UiHandle;
    fn widget_set_label(&self, handle: UiHandle, label: &str);
    fn widget_set_callback(&self, handle: UiHandle, fn_ptr: i64);
    fn app_run(&self);
    // ...
}
```

Os `extern "C"` delegam para o backend selecionado via feature flag:

```toml
[features]
default = ["ui-fltk"]
ui-fltk    = ["dep:rts-ui-fltk"]
ui-webview = ["dep:rts-ui-webview"]   # mobile
```

| Plataforma | Backend | Crate |
|---|---|---|
| Windows / Linux | FLTK | `fltk` (bundled) |
| macOS | FLTK ou Slint | `fltk` / `slint` |
| iOS | WebView | `wry` (WKWebView) |
| Android | WebView | `wry` (Android WebView) |

Código TS do usuário permanece idêntico em todas as plataformas.

---

### P3 — async/await nativo

**Motivação:** `async/await` é a feature mais solicitada. Depende de:
1. Representação de `Promise<T>` como handle no GC
2. Scheduler de microtasks (fila por thread)
3. Codegen de state machine para funções `async` (semelhante a generators)

**Estimativa:** ~3 meses de trabalho.

---

### P4 — Generators / yield

Depende da mesma infra de state machine de P3. Pode ser implementado em paralelo.

---

### P5 — LSP server

`rts lsp` — servidor Language Server Protocol para VS Code / Neovim.
Depende de:
- Source maps (PC → linha TS) no `.ometa`
- Type system com inferência suficiente para hover types
- Go-to-definition via grafo de módulos

---

### P6 — `rts check`

Type-check sem compilar. Essencial para CI. Roda o parser + type_system e emite diagnósticos sem passar pelo codegen.

---

### P7 — Proxy / Reflect / WeakMap / Symbol

Extensões de linguagem JS que faltam. Menor prioridade que async.

---

## Backlog de features de linguagem

| Feature | Status | Issue |
|---|---|---|
| async/await | pendente | — |
| Generators / yield | pendente | — |
| Proxy / Reflect | pendente | — |
| WeakMap / WeakRef | pendente | — |
| Symbol | pendente | — |
| Dynamic import() | pendente | — |
| Closures com env-record real | parcial | #97 |
| DWARF debug info | pendente | #90 |
| Loop vectorization | inviável sem vectorizer Cranelift | #92 fechada |

---

## Convenção de atualização

- Ao concluir um plano: mover para seção "Concluído" com data e benchmark real
- Ao descobrir novo problema: abrir issue + adicionar aqui se bloquear plano existente
- Benchmarks reais substituem estimativas quando medidos
