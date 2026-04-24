# AGENT.md

Este arquivo e um stub. A referencia completa de arquitetura, convencoes e contrato ABI
vive em `CLAUDE.md` na raiz do repositorio.

Agentes automatizados e humanos devem consultar, nesta ordem:

1. `CLAUDE.md` — arquitetura, pipeline, contrato ABI, convencoes de commit e de namespaces
2. `NEXT_STEPS.md` — direcao vigente da branch
3. `ROAD_MAP.md` — plano de medio prazo
4. `docs/specs/INDEX.md` — especificacoes de features e decisoes de design

## Notas especificas para agentes

- Namespaces ativos: `io`, `fs`, `gc`, `math`, `bigfloat`.
- Contrato ABI unico vive em `src/abi/` (`SPECS`, `NamespaceMember`, `AbiType`, `Intrinsic`,
  simbolos `__RTS_FN_NS_<NS>_<NAME>` e dados `__RTS_DATA_NS_<NS>_<NAME>`). Nao ha mais
  `dispatch()` por namespace, nem `JsValue` no limite, nem `__rts_call_dispatch`.
- Sem HIR/MIR separados: codegen consome AST direto em `src/codegen/lower/`.
- Dois paths de execucao: `ObjectModule` (AOT, default) e `JITModule` (opt-in via
  `RTS_JIT=1`). `FnCtx.module` e `&mut dyn Module` — ambos passam pelo mesmo codegen.
- Build e via `cargo` puro (sem `xtask`). Artefatos do usuario ficam em `node_modules/.rts/`.
- `Intrinsic` em `NamespaceMember.intrinsic` permite emitir IR inline em vez de `call`;
  ver `lower_intrinsic` em `src/codegen/lower/expr.rs`.
- User functions usam `CallConv::Tail` (para TCO); `__RTS_MAIN` usa platform default
  porque e chamado por C extern.
- Namespaces removidos (net, process, crypto, buffer, promise, task, global) serao
  reintroduzidos sobre o contrato novo — nao assumir que estao disponiveis.

Qualquer duplicacao de conteudo entre este arquivo e `CLAUDE.md` deve ser resolvida
a favor de `CLAUDE.md`.
