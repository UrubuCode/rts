# RTS - Next Steps

## Direcao alvo

Seguir a mesma ideia arquitetural da `main`, mas mantendo a API nova organizada.

Isso significa:
- pipeline completo com grafo de modulos, cache de objetos e link final;
- runtime support integrado ao `rts` (sem dependencia de `rts.lib` externa);
- sem download de runtime lib em tempo de uso;
- sem fallback para `cargo build --lib` no ambiente do usuario;
- API de runtime centralizada em `src/abi/` e namespaces organizados por modulo.

## Base funcional desejada (paridade com main)

1. Compilacao por grafo (`ModuleGraph`) e nao apenas arquivo unico.
2. Cache incremental de `.o` + metadata (`.ometa`) por modulo.
3. Link final via backend de sistema por padrao (`system_linker`) com fallback quando necessario.
4. Resolucao de runtime support a partir de payload interno do proprio `rts`.
5. Emissao e sincronizacao de artefatos em `node_modules/.rts`.
6. Distribuicao standalone: uso de `rts` fora do repo sem `Cargo.toml`.

## API nova organizada (mantida)

- `src/abi/` como contrato unico de ABI:
  - `member`, `types`, `signature`, `symbols`, `guards`.
- `src/abi/mod.rs::SPECS` como registro oficial dos namespaces.
- Namespaces em `src/namespaces/<ns>/` com separacao por responsabilidade:
  - `abi.rs` para declaracao de membros;
  - arquivos operacionais (`read.rs`, `write.rs`, `ops.rs`, etc.) para implementacao.
- Codegen consultando `SPECS` para resolver simbolo + assinatura de chamada.

## Plano de execucao

### Etapa 1 - Reancorar no fluxo da main

- Reintroduzir pipeline de grafo/caching inspirado em `origin/main`.
- Substituir `runtime_lib` externo por runtime payload interno ao `rts`.
- Remover caminho de download de runtime support library.
- Remover fallback para `cargo build --lib` no fluxo de execucao do usuario.
- Manter o linker atual e validar compilacao end-to-end de exemplos.

### Etapa 2 - Plugar API nova no pipeline completo

- Trocar tabelas antigas de dispatch pelo registro em `abi::SPECS`.
- Garantir que `io`, `fs`, `gc` funcionem no fluxo completo de modulos.
- Gerar/atualizar declaracoes TypeScript a partir dos specs da ABI nova.

### Etapa 3 - Migracao incremental dos recursos da bench nova

- Trazer melhorias de codegen em lotes pequenos.
- Medir impacto por lote (tempo de compile, tamanho de binario, benchmark).
- Nao misturar refatoracao estrutural com mudanca de semantica no mesmo lote.

## Criterios de pronto para a proxima fase

- `rts compile` funciona com pipeline de grafo e cache.
- `rts run` e `rts compile` validos em exemplos principais.
- `rts run` funciona fora do repo sem `Cargo.toml`, sem `rts.lib` externa.
- `io/fs/gc` estaveis no contrato novo da ABI.
- build e docs sem dependencia de `xtask`.
