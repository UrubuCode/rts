# Guia: como criar um novo namespace

Este documento descreve o processo atual (branch `feat/remake-namespaces`) para
adicionar um novo namespace de runtime ao RTS. O contrato entre codegen e
runtime vive em `src/abi/` e nao depende mais de `dispatch()` por namespace,
`JsValue`, `MEMBERS`/`SPEC` soltos em cada `mod.rs` ou do router central
`__rts_call_dispatch`.

## 1. Visao geral — contrato unico via `src/abi/`

Todos os namespaces sao descritos por tabelas estaticas com o mesmo formato.
Codegen resolve chamadas atraves desse contrato e emite `call <simbolo>`
direto para funcoes `#[unsafe(no_mangle)] pub extern "C"` expostas pelo
runtime.

Arquivos que definem o contrato:

- `src/abi/mod.rs` — re-exports e a const `SPECS: &[&NamespaceSpec]` com a
  lista global de namespaces registrados. Tambem expoe `abi::lookup("ns.fn")`
  usado pelo codegen.
- `src/abi/member.rs` — tipos `NamespaceSpec`, `NamespaceMember`, `MemberKind`.
- `src/abi/types.rs` — enum `AbiType` com as primitivas da borda C.
- `src/abi/symbols.rs` — macro `rts_sym!` (compile-time) e `validate_symbol`
  usado nos testes.

Principio: **nenhum valor polimorfico cruza a borda**. Cada membro declara
tipos primitivos que mapeiam 1:1 para o ABI C; string dinamica volta como
handle `u64` gerenciado pela GC.

## 2. Layout de arquivos do namespace

```
src/namespaces/<ns>/
  mod.rs       — import map: re-exporta submodulos e expoe `pub mod abi;`
  abi.rs       — tabela estatica: MEMBERS + SPEC
  <grupo>.rs   — implementacao: funcoes #[unsafe(no_mangle)] pub extern "C"
```

`mod.rs` nao carrega logica. Cada arquivo operacional agrupa funcoes por
protocolo ou responsabilidade (ex.: `tcp.rs`, `udp.rs`, `read.rs`,
`metadata.rs`). Utilitarios compartilhados entre namespaces vao em
`src/namespaces/utils/<ns>.rs`.

## 3. Checklist para adicionar um novo namespace

1. Criar a pasta `src/namespaces/<ns>/`.
2. Criar `mod.rs` com `pub mod abi;` e `pub mod <grupos>;`.
3. Criar `abi.rs` com:
   - `pub const MEMBERS: &[NamespaceMember] = &[ ... ];`
   - `pub const SPEC: NamespaceSpec = NamespaceSpec { name, doc, members: MEMBERS };`
4. Implementar cada funcao em seu arquivo de grupo como
   `#[unsafe(no_mangle)] pub extern "C" fn __RTS_FN_NS_<NS>_<NAME>(...)`.
5. Registrar o namespace em `src/namespaces/mod.rs` (`pub mod <ns>;`).
6. Adicionar `&crate::namespaces::<ns>::abi::SPEC` em `abi::SPECS`
   (`src/abi/mod.rs`).
7. Rodar `cargo test` — a validacao de simbolos em `abi::symbols` roda
   implicitamente sobre `SPECS` e rejeita nomes malformados.
8. Rodar `target/release/rts.exe apis` e inspecionar o `rts.d.ts` gerado por
   `rts init` para confirmar que a superficie publica esta correta.

Nao ha `dispatch()` para implementar, nao ha `RTS_EXPORTS` para estender,
nao ha fallback por `JsValue`. A unica entrada e o par
`(SPEC em SPECS, simbolo exportado)`.

## 4. Exemplo minimo — modelo baseado em `io`

`src/namespaces/demo/mod.rs`:

```rust
//! `demo` namespace — exemplo minimo.

pub mod abi;
pub mod ops;
```

`src/namespaces/demo/abi.rs`:

```rust
use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "echo",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_DEMO_ECHO",
        args: &[AbiType::StrPtr],
        returns: AbiType::Void,
        doc: "Escreve a mensagem recebida em stdout.",
        ts_signature: "echo(message: string): void",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "demo",
    doc: "Namespace de exemplo.",
    members: MEMBERS,
};
```

`src/namespaces/demo/ops.rs`:

```rust
use std::io::{self, Write};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DEMO_ECHO(ptr: *const u8, len: i64) {
    if ptr.is_null() || len < 0 {
        return;
    }
    // SAFETY: contrato do ABI garante ptr+len validos durante a chamada.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(slice);
    let _ = lock.write_all(b"\n");
}
```

Registro em `src/abi/mod.rs`:

```rust
pub const SPECS: &[&NamespaceSpec] = &[
    &crate::namespaces::gc::abi::SPEC,
    &crate::namespaces::io::abi::SPEC,
    &crate::namespaces::fs::abi::SPEC,
    &crate::namespaces::demo::abi::SPEC,
];
```

O codegen agora resolve `demo.echo` via `abi::lookup("demo.echo")` e emite
`call __RTS_FN_NS_DEMO_ECHO` direto.

## 5. Convencao ABI por tipo

`AbiType` (em `src/abi/types.rs`) enumera as primitivas permitidas na borda.
`StrPtr` e o unico tipo composto: expande em dois slots Cranelift
`(ptr: i64, len: i64)`.

| `AbiType` | Tipo Rust na funcao `extern "C"` | Slots | Uso tipico                        |
|-----------|----------------------------------|-------|-----------------------------------|
| `Void`    | `()`                             | 0     | retorno sem valor                 |
| `Bool`    | `i64` (0 ou 1)                   | 1     | flags                              |
| `I32`     | `i32`                            | 1     | inteiro pequeno                   |
| `I64`     | `i64`                            | 1     | inteiro / codigo de erro          |
| `U64`     | `u64`                            | 1     | ponteiro/offset opaco, handle     |
| `F64`     | `f64`                            | 1     | numero JS por padrao              |
| `StrPtr`  | `*const u8, i64`                 | 2     | string estatica do codegen (UTF-8)|
| `Handle`  | `u64` (gen:16 + slot:48)         | 1     | recurso heap via `HandleTable`    |

Regras complementares:

- `StrPtr` nao e retornavel (`is_returnable() == false`). Para retornar
  string dinamica use `Handle`: aloque via o namespace `gc` (ex.:
  `__RTS_FN_NS_GC_STRING_NEW`) e devolva o handle. O chamador recupera dados
  com `gc.string_ptr(h)` / `gc.string_len(h)`.
- Handles sao gerenciados por `namespaces::gc::handles::HandleTable` — nunca
  invente um espaco de handles proprio.
- Simbolos seguem `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` validado por
  `abi::symbols::validate_symbol`. Para funcoes de namespace use sempre
  `__RTS_FN_NS_<NS>_<NAME>` (uppercase ASCII, digitos e `_`).
- A macro `rts_sym!` em `src/abi/symbols.rs` gera o simbolo em compile-time
  (`rts_sym!(FN NS IO PRINT)` -> `"__RTS_FN_NS_IO_PRINT"`).

## 6. Registro em `abi::SPECS`

A ordem em `SPECS` e estavel e significativa para reprodutibilidade do
codegen. Preserve ordem alfabetica ou a ja existente — nao reordene por
conveniencia. Cada entrada e um `&'static NamespaceSpec`, sem alocacao em
heap.

`abi::lookup("<ns>.<fn>")` resolve em `O(n*m)` sobre tabelas pequenas e
retorna `(&NamespaceSpec, &NamespaceMember)` para o codegen emitir a
chamada tipada.

## 7. Geracao de `rts.d.ts`

O arquivo `rts.d.ts` e produzido por `emit_rts_dts` em
`src/cli/init.rs`, que itera `abi::SPECS` e usa, para cada membro, o campo
`doc` e o campo `ts_signature` — escreva-os de forma que o usuario final
entenda a API sem precisar abrir o Rust. Constantes (`MemberKind::Constant`)
viram `export const`, funcoes viram `export function`.

Nao ha outro arquivo gerado; nao adicione modulos adicionais ao
`declare module "rts"`.

## 8. Testes esperados

- `cargo test` — cobre a validacao de simbolos em `src/abi/tests` e os testes
  proprios do namespace.
- `cargo build --release` — o build nao pode terminar com warnings de
  `dead_code`.
- `target/release/rts.exe apis` — lista o namespace novo com suas
  assinaturas.
- Quando a API for observavel por TS, adicionar um teste integrado em
  `tests/` que chame `<ns>.<fn>` atraves de `rts run`.

## 9. Estendendo o `HandleTable` (recursos heap)

Recursos que vivem alem de uma unica chamada (sockets, arquivos abertos,
buffers, objetos) usam sempre `namespaces::gc::handles::HandleTable`. Nao
crie tabela paralela — o codegen depende desse espaco unico para validar
generation+slot de handles.

Para suportar um novo tipo de recurso:

1. Em `src/namespaces/gc/handles.rs` adicione uma variant ao `enum Entry`,
   por ex. `Entry::TcpStream(std::net::TcpStream)` ou `Entry::Buffer(Vec<u8>)`.
2. No seu namespace, use o padrao "lock curto":

   ```rust
   use crate::namespaces::gc::handles::{Entry, table};

   #[unsafe(no_mangle)]
   pub extern "C" fn __RTS_FN_NS_<NS>_<NAME>_NEW(size: i64) -> u64 {
       if size < 0 { return 0; }
       let bytes = vec![0u8; size as usize];
       let mut t = table().lock().expect("handle table poisoned");
       t.alloc(Entry::Buffer(bytes))
   }
   ```

3. Toda funcao que consome um handle deve validar via `t.get(handle)` e
   rejeitar handles invalidos com um sentinela (`-1`, `0`, null) em vez de
   panicar. Handles invalidos sao parte do contrato, nao bug.
4. Se o recurso precisa de leitura nativa direta (ex.: `load`/`store` em
   campo de objeto), exponha `<NS>_<NAME>_PTR(handle) -> U64` no modelo de
   `__RTS_FN_NS_GC_OBJECT_PTR` — o codegen pega o ponteiro e emite
   `load`/`store` diretos, evitando round-trip ABI por acesso.

Lembre-se: `Entry::Free` nunca deve ser retornado por `get()`; a
implementacao atual ja garante isso.

## 10. Armadilhas comuns (aprendizado de implementacao)

- **Cache de runtime support invalida.** Adicionar um simbolo novo
  (`__RTS_FN_NS_<NS>_<NAME>`) nao invalida objetos ja materializados em
  `~/.rts/toolchains/rts-runtime-objects/<target>/<hash>/`. Se o linker
  acusar "undefined symbol", apague a pasta do target e rode qualquer
  `rts run` — o build do runtime support roda de novo e regenera o hash.
- **`Box::leak` para simbolos dinamicos.** Quando o simbolo depende de
  nomes do codigo do usuario (ex.: `__RTS_USER_<Class>_<method>`), e
  aceitavel usar `Box::leak(format!(...).into_boxed_str())` para obter
  `&'static str`. O conjunto cresce so durante a compilacao e some com o
  processo. Nao use isso para simbolos dos namespaces built-in — esses
  sao literais estaticos em `abi.rs`.
- **`StrPtr` nao e retornavel.** Se precisa devolver string, aloque um
  `Handle` via `gc::string_new`. Funcoes que retornam `StrPtr` nao
  compilam (`lower_member` rejeita).
- **`AbiType::Handle` vs `AbiType::U64`.** Os dois sao `u64` no ABI C,
  mas sinalizam intencoes diferentes: `Handle` e opaco e validado contra
  generation; `U64` e numero/ponteiro bruto. Codegen e guards tratam
  nullish de forma diferente (Handle rejeita, U64 coerce). Escolha
  conscientemente.
- **Coercoes de argumento sao responsabilidade do codegen**, nao da
  funcao extern. Quando `io.print(42)` chega no extern, o codegen ja
  converteu o `42` para handle de string via `gc::string_from_i64`. Sua
  funcao recebe tipos exatos do `AbiType` declarado e nao faz coercao.
- **Nao retorne `Result<T>` atravessando a ABI.** Use convencao de
  sentinela (`-1`, `0` para handle invalido) e opcionalmente um par
  `ok/err` exposto como dois simbolos separados. `Result<T>` existe como
  ergonomia em TS (builtin packages), mas a borda C e crua.
- **Estado compartilhado.** Quando o namespace precisa de estado mutavel
  global, siga o padrao do `HandleTable`: `static LOCK:
  OnceLock<Mutex<State>>` com funcao pub(super) `fn state() -> &'static
  Mutex<State>`. Thread-local so para caches por-thread.
