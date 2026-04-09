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

Cada namespace gerencia seu proprio state interno com `OnceLock<Mutex<...>>`.
**NAO colocar state de namespace em `src/namespaces/state.rs`** — esse arquivo e para state global compartilhado (buffers, promises, globals).

Exemplo (dentro de `src/namespaces/<name>/mod.rs`):

```rust
struct MyState {
    handles: BTreeMap<u64, MyHandle>,
    next_id: u64,
}

static MY_STATE: OnceLock<Mutex<MyState>> = OnceLock::new();

fn lock_my() -> MutexGuard<'static, MyState> {
    let state = MY_STATE.get_or_init(|| Mutex::new(MyState::default()));
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
```

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
