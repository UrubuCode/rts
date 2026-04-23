# RTS - Next Steps

## Direcao alvo

Seguir a mesma ideia arquitetural da `main`, mas mantendo a API nova organizada.

Isso significa:
- pipeline completo com grafo de modulos, cache de objetos e link final;
- runtime support library resolvida como artefato externo (nao embed recursivo no build);
- API de runtime centralizada em `src/abi/` e namespaces organizados por modulo.

## Base funcional desejada (paridade com main)

1. Compilacao por grafo (`ModuleGraph`) e nao apenas arquivo unico.
2. Cache incremental de `.o` + metadata (`.ometa`) por modulo.
3. Link final via backend de sistema por padrao (`system_linker`) com fallback quando necessario.
4. Resolucao de runtime support library no estilo `runtime_lib` da `main`.
5. Emissao e sincronizacao de artefatos em `node_modules/.rts`.

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
- Reestabelecer `runtime_lib` como fonte de runtime support library.
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
- `io/fs/gc` estaveis no contrato novo da ABI.
- build e docs sem dependencia de `xtask`.
