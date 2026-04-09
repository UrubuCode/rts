# Guia: Como criar um namespace runtime

Checklist obrigatorio ao adicionar novo namespace ao modulo `"rts"`.

## 1. Criar o modulo

Arquivo: `src/namespaces/<name>/mod.rs`

```rust
const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "funcao",
        callee: "<name>.funcao",
        doc: "Descricao.",
        ts_signature: "funcao(arg: str): void",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "<name>",
    doc: "Descricao do namespace.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "<name>.funcao" => { /* ... */ }
        _ => None,
    }
}
```

## 2. Registrar no mod.rs central

Arquivo: `src/namespaces/mod.rs`

- Adicionar `pub mod <name>;` nos imports
- Adicionar `<name>::SPEC` ao array `SPECS`
- Adicionar `<name>::dispatch(callee, args)` na chain `.or_else()` da fn `dispatch()`

## 3. Adicionar aos exports do builtin "rts"

Arquivo: `src/runtime/mod.rs`

- Adicionar `"<name>"` ao array `RTS_EXPORTS`

**CRITICO: sem isso, `import { <name> } from "rts"` falha no type checker em runtime, nao em compilacao Rust.**

## 4. State (se precisar de handles/recursos)

Usar o state centralizado em `src/namespaces/state/` via imports:

```rust
use crate::namespaces::state::{State, Mutex};
```

Registrar um Mutex nomeado para o namespace:

```rust
fn lock_my() -> std::sync::MutexGuard<'static, MyState> {
    let state = Mutex.get_or_init("my_namespace", Mutex::new(MyState::default()));
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
```

**REGRAS:**
- **NAO criar `OnceLock`/`Mutex` soltos dentro do namespace** — sempre via state centralizado
- **NAO adicionar logica de namespace dentro de `state/*.rs`** — state so expoe primitivas (Mutex, State, Globals)
- Toda alocacao de recurso passa pelo state para rastreamento futuro do GC

## 5. Regenerar rts.d.ts

- Rodar qualquer `rts build` no diretorio raiz do projeto
- `sync_namespace_artifacts()` gera automaticamente o `.d.ts` a partir dos SPECS

## 6. Testar

```bash
cargo build --release                           # compila sem erros
target/release/rts.exe run test_<name>.ts        # funciona interpretado
target/release/rts.exe build -p test.ts output   # funciona nativo
target/release/rts.exe apis                      # lista o namespace
```
