# PASSO 4 — Debug Info, `.ometa`, `CompileMode` e detecção `.env`

## Objetivo

1. Adicionar detecção de modo via `.env` a `CompilationProfile` (RTS_MODE, NODE_ENV, APP_ENV).
2. Criar `src/namespaces/rust/debug.rs` com operações de debug info em runtime.
3. Criar `src/codegen/cranelift/ometa.rs` com o writer do formato `.ometa`.

## Por que `.ometa` separado do DWARF?

`.ometa` é lido pelo runtime C em microsegundos (JSON simples). DWARF é lido por ferramentas
externas (gdb, lldb). Para erros em desenvolvimento, `.ometa` é mais rápido que parsear DWARF.

## 1. compile_options.rs — detecção `.env`

Adicionar `CompilationProfile::from_env(project_root: &Path) -> Self`:
- Lê `.env` no diretório do projeto
- Prioridade: RTS_MODE > NODE_ENV > APP_ENV
- Fallback: Development se nenhuma variável definida

## 2. src/namespaces/rust/debug.rs

Primitivas de debug info disponíveis em runtime:

- `rts.debug.load_metadata(path_ptr: u64) -> u64` — carrega .ometa, retorna handle
- `rts.debug.resolve_location(handle: u64, pc_offset: u64) -> u64` — resolve PC → source location
- `rts.debug.format_error(message_ptr: u64, pc_offset: u64) -> u64` — formata erro com localização

Estado: `OnceLock<Arc<Mutex<HashMap<u64, DebugMetadata>>>>` — lazy load por handle.

## 3. src/codegen/cranelift/ometa.rs

Estrutura do `.ometa`:

```json
{
  "version": 1,
  "mode": "development",
  "sourceRoot": "/project/src",
  "sources": ["index.ts"],
  "locations": {
    "0x12a3f": { "source": "index.ts", "line": 42, "column": 10 }
  },
  "functions": {
    "_rts_foo": { "offset": 74751, "size": 256, "source": "index.ts", "line": 40 }
  }
}
```

`OmetaWriter` — acumula localizações durante codegen, serializa ao final.

## Registro

`debug.rs`: adicionar `DEBUG_MEMBERS` e `DEBUG_SPEC` em `rust/mod.rs`.
`ometa.rs`: usado pelo codegen (não é namespace, não vai em SPECS).
