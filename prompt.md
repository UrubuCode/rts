# Handoff — migração `rts-primitives → .ts` no motor novo

> Documento de retomada. Branch `feat/rts-codegen-new`. Idioma: pt-BR.
> Leia **antes** `CLAUDE.md` + `.claude/rules/00-meta.md` … `05-codegen-notes.md`
> + `docs/specs/rts-codegen-new-design.md` (canônico). Este arquivo resume o
> estado e o padrão exato a seguir.

---

## 1. Contexto

Motor novo = `crates/rts-codegen-new` (TS→Cranelift, PolyValue NaN-box + shapes +
ICs). Motor velho `crates/rts-codegen-old` **congelado**, ainda plugado no
bin/CLI até o cutover. O motor novo roda `.ts` real via `run_source(src)` (string)
e `run_path(entry)` (multi-arquivo); CLI: **`rts run-new <file>`**.

**Objetivo da campanha (dono):** tirar TUDO que não é primitivo de dentro do
motor. O motor só pode nomear/hardcodar os **primitivos com sintaxe nativa**:
`string` `""`, `number` `123`, `boolean` `true`, `array` `[]`, `object` `{}`,
`function`, template, regex `/re/`, `Error`+subclasses (primordiais). A
**biblioteca de métodos** de cada um vira `.ts` no prelude; o **core irredutível**
(formatação, ops Unicode) fica em Rust e é re-exposto **privadamente** via o
namespace `rts:engine`. Nenhum builtin novo no motor. Doutrina
PRIMORDIAL-vs-Registry em `CLAUDE.md` (mandatória).

**Já migrado (sessão anterior, 9 commits, 662 unit tests, 0 regressão):**
`Error`+7 subclasses, `Boolean`, `Number`, `String` (21 métodos). Mais: namespace
privado `rts:engine`, sistema de **módulos ES** multi-arquivo, **builtin-import**
(`import {x} from "rts:io"`).

---

## 2. O PADRÃO PROVADO (siga exatamente para Object/Function/Array)

Migrar a superfície de um primitivo/classe para `.ts`:

1. **Classe `.ts`** em `crates/rts-primitives/src/<nome>.ts` — só protótipo
   (métodos), corpo chama helpers privados `engine.*` onde precisa de op
   irredutível; valueOf/lógica pura direto em TS. Modelo: `error.ts`,
   `boolean.ts`, `number.ts`, `string.ts`.
2. **Const + facade:** `pub const <NOME>_TS: &str = include_str!("<nome>.ts");`
   em `crates/rts-primitives/src/lib.rs`; re-export em `crates/rts-runtime/src/lib.rs`.
3. **Include no prelude:** `e.include(<NOME>_TS);` em
   `crates/rts-codegen-new/src/front/run/registry.rs::build_registry()`
   (ordem importa — base antes de subclasse; Error já é o 1º).
4. **Helpers irredutíveis** (se preciso): adicione membros ao namespace privado
   `rts:engine` (`crates/rts-std/src/engine/mod.rs` + arquivos por família, ex.
   `engine/string.rs`) que **ENVOLVEM** o impl Rust existente (`__RTS_FN_GL_*` /
   `__RTS_FN_NS_*`) — chamar, nunca reimplementar (1 fonte de verdade). Lowering
   em `crates/rts-codegen-new/src/front/run/engineobj.rs` (tabela `EngineStr`/
   `lower_engine_str` é table-driven: adicionar membro = linha de dado). Registrar
   sig em `value/abi_sig.rs` + símbolo JIT em `runtime_link.rs`.
5. **Rota de dispatch:** em `crates/rts-codegen-new/src/front/run/method.rs`,
   roteie o receiver PRIMITIVO provado para a classe `.ts` via
   `try_primitive_class_method(recv, "<Classe>", method, args)` **ANTES** de
   `dispatch.rs::resolve_method`. Esse helper boxa o primitivo como `this` e chama
   `call_synth_fn` (mesmo caminho de instância de classe). **NÃO** usa protótipo
   JS — o motor é shape-based.
6. **Drene as rows** já cobertas pela `.ts` em `crates/rts-codegen-new/src/dispatch.rs`
   (`STRING_ROWS`/`NUMBER_ROWS`/…). Tabela vazia ⇒ método não-coberto ainda
   **BAILA** explicitamente (sem chute). O que a `.ts` ainda não cobre, MANTENHA
   na row (honesto) e anote como follow-up.
7. **Mantenha pro motor velho:** `register_*_class_spec`, os `__RTS_FN_GL_*` Rust,
   e o wrapper `new X(x)` (`typeof "object"`). Carve-outs já existem:
   `is_wrapper_primordial` (ctor wrapper), `is_wrapper_primordial_static`
   (estáticas tipo `Number.isNaN`), `is_global_static_class` (ex.
   `String.fromCharCode`) — a classe `.ts` ambiente é transparente a esses paths.

---

## 3. Namespace privado `rts:engine`

`crates/rts-std/src/engine/mod.rs` (+ `string.rs`). Membros: arch, time
(now_ms/ns, unix_ms/ns), trace (push/pop/capture/print) — usados por Error.stack
e Date; `num_*` (toFixed/toPrecision/toExponential/toString-radix); `str_*` (21
ops). Marcado `.private()`. **Gate de privacidade:** `Lowerer::is_prelude` — só
função origem-prelude pode nomear o global `engine`; código de usuário que nomeia
`engine.*` = `Unsupported` explícito. Lowering: `front/run/engineobj.rs`.

Quando uma op irredutível faltar para Object/Function: adicione um `engine.*`
privado que envolve o impl Rust (não invente builtin no motor).

---

## 4. Sistema de módulos

- **ES (FEITO, M1):** `crates/rts-codegen-new/src/front/modules/` (resolver/grafo
  BFS, cycle-detect, flatten, branch builtin `rts:`/`node:`). `run_path(entry)` em
  `front/run/module_entry.rs`. CLI `rts run-new`. Multi-arquivo `./imports` roda
  e2e. Erros explícitos: colisão de nome top-level, ciclo, import não-exportado,
  npm-bare, `export * as`.
- **builtin-import (FEITO):** `import {x} from "rts:io"` resolve via
  `registry.rs::namespace_member` (io+math wired) = std io público importável.
- **GAP:** `import * as ns` é dropado no parser (`rts-parser/src/lowering_items.rs`).
- **CommonJS (PENDENTE, M2):** `module.exports`/`exports.name`/`require` — swc não
  parseia como módulo; precisa trabalho no parser (reconhecer os padrões
  assign/call) + interop ES.

---

## 5. Trabalho restante (ordem sugerida, mesmo padrão da §2)

1. **Object → .ts** — métodos de instância (`hasOwnProperty`, `toString`,
   `valueOf`, `isPrototypeOf`…). Estáticas (`keys/values/entries/assign/...`) já
   passam por `front/run/mathobj.rs`/Registry — confira o que falta.
2. **Function → .ts** — `call`/`apply`/`bind`/`toString` + `name`/`length`.
   Cuidado: function values têm ABI 4-posicional+rest (`value/funcops.rs`).
3. **Array-methods → .ts** — ATENÇÃO à impedância: elemento de array é `PolyValue`
   em `Entry::Vec` (motor novo) vs o Array runtime que lê i64 cru. Métodos já no
   motor: `value/arrayops.rs` + `arraycb.rs` (callbacks). Avalie o que dá pra
   mover pra `.ts` sobre primitivos vs manter.
4. **console → .ts** — tirar `lower_console_log`/`is_console_ident` de
   `front/run/call.rs`. **COMPLICAÇÃO:** `io.print` escreve stdout REAL, mas
   `console.log` usa um **sink de captura** separado (testes). `rts:engine`
   precisa ganhar stdout/stderr/stdin + `inspect(value)->string` (envolve o
   `__rtsadp_inspect` que o motor já tem) roteando pro MESMO sink; console.ts
   precisa de rest-params `...args` + `.join`. **Investigar o sink primeiro.** Não
   pode regredir os 662 testes que usam `console.log`.
5. **CommonJS (M2)** — ver §4.

---

## 6. Exigências e padrões (BINDING)

- **Você coordena; use AGENTS** para executar as tarefas (o dono é a cabeça).
  Disparar agents de investigação em paralelo, depois de implementação.
- **Todo arquivo `.rs` < 500 linhas**, split por pasta/subpasta.
- **JAMAIS hardcode no motor** além dos primitivos (string/number/array/object/
  function/template/regex/Error). Tudo não-primitivo: Registry ou `.ts`.
- **Honesty floor (nunca afrouxa):** nenhum fixture deletado/desabilitado/
  hardcoded p/ inflar métrica; nada que crasha/trava commitado como "pass"; build
  SEMPRE compila. Fixture passa só pelo mesmo caminho que qualquer input usaria.
- **Sem regressão silenciosa** (`REGRESS WHEN NECESSARY` em `00-meta.md`):
  regressão só explícita + justificada no commit. Baseline atual do motor novo:
  **662 unit tests**. Motor velho: TS 1710/1710 + cross-runtime baseline — qualquer
  mudança em crate compartilhado (rts-hir/rts-ast/rts-parser/rts-primitives/
  rts-shared/rts-engine) exige GATE (não regredir o velho).
- **Não quebrar o motor velho:** ele está no CLI. Mantenha `__RTS_FN_GL_*`,
  `register_*_class_spec`, wrappers `new X()`. Drene só do MOTOR NOVO.
- **Sem código morto / comentado** (regra "No legacy code"). `todo!()` ok como WIP.
- **Atualize o design doc** (`docs/specs/rts-codegen-new-design.md`) no mesmo
  commit quando mudar arquitetura — nunca deixe a spec mentindo.
- **Commits:** conventional (`feat:`/`fix:`/`chore:`/`refactor:`/`docs:`), corpo em
  pt-BR explicando o "porquê", footer
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
  Use heredoc no Bash (`git commit -F - <<'EOF'`), NÃO here-string PowerShell.

---

## 7. Como testar

```bash
# unit do motor novo (rápido, use durante dev)
cargo test -p rts-codegen-new --lib 2>&1 | tail -10
# build incremental do motor + CLI
cargo build -p rts-codegen-new -p rts-cli 2>&1 | tail -8
# rodar um .ts real pelo motor novo (multi-arquivo resolve ./imports)
cargo build --release -p rts-runtime && cargo build --release
target/release/rts.exe run-new caminho/arquivo.ts
# GATE motor velho (quando tocar crate compartilhado)
target/release/rts.exe test            # suíte TS (1710/1710)
```

Testes do motor novo: `crates/rts-codegen-new/src/front/run/tests/*.rs`
(`error_class.rs`/`boolean_class.rs`/`number_class.rs`/`string_class.rs` =
modelos; `modules_e2e.rs`/`builtin_import.rs` = harness de temp-dir + `run_path`).
Registrar cada arquivo novo em `tests/mod.rs`. Asserir stdout capturado REAL.

---

## 8. Mapa rápido de arquivos

| Papel | Caminho |
|---|---|
| Rota primitivo→.ts | `crates/rts-codegen-new/src/front/run/method.rs` (`try_primitive_class_method`) |
| Tabela de dispatch (drenar rows) | `crates/rts-codegen-new/src/dispatch.rs` |
| Registry + include do prelude | `crates/rts-codegen-new/src/front/run/registry.rs` |
| Namespace privado engine (impl) | `crates/rts-std/src/engine/{mod,string}.rs` |
| Lowering `engine.*` | `crates/rts-codegen-new/src/front/run/engineobj.rs` |
| Sigs ABI + símbolos JIT | `crates/rts-codegen-new/src/value/abi_sig.rs`, `src/runtime_link.rs` |
| Classes .ts já migradas | `crates/rts-primitives/src/{error,boolean,number,string}.ts` |
| Consts .ts + facade | `crates/rts-primitives/src/lib.rs`, `crates/rts-runtime/src/lib.rs` |
| Módulos (resolver/grafo) | `crates/rts-codegen-new/src/front/modules/`, `front/run/module_entry.rs` |
| CLI motor novo | `crates/rts-cli/src/cli/run_new.rs` (`rts run-new`) |
| console hardcoded (a remover) | `crates/rts-codegen-new/src/front/run/call.rs` (`lower_console_log`) |
| Carve-outs wrapper/estática | `method.rs`/`globalclass.rs`/`globals.rs`/`mathobj.rs` |

---

## 9. Gaps/limitações conhecidos (não são regressões — anotados)

- `import * as ns` dropado no parser.
- `e.toString()` num Error capturado **opaco** cai no default genérico (precisa
  dispatch dinâmico shape-keyed = incremento IC). `new Error("x").toString()`
  (receiver estaticamente classificado) funciona.
- String: `split`/`length`/`codePointAt`/`localeCompare`/`substr`/regex-first
  mantidos no path do motor (não drenados).
- Divergência de surrogate conhecida no impl Rust de `charAt`/`.at` — NÃO mexer.
- `console.log` de objeto/array inteiro: pretty-print fiel ainda é incremento.

---

**Resumo de uma linha:** continue drenando primitivos do motor para `.ts` pelo
padrão da §2 (Object → Function → Array → console → CommonJS), via agents,
arquivos <500, sem hardcode novo no motor, sem regredir 662 + motor velho, design
doc sincronizado.
