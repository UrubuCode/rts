# RTS - Next Steps

## Status atual

Etapa 1 (reancorar) e Etapa 2 (plugar API nova) estao **concluidas** — pipeline por grafo,
cache incremental, runtime support embutido, `abi::SPECS` como fonte unica, namespaces core
(`io`/`fs`/`gc`) estaveis e novos namespaces adicionados.

Etapa 3 (migracao incremental de melhorias de codegen + expansao de namespaces) em andamento —
ver **Progresso recente** abaixo.

## Progresso recente

### Codegen + capacidades TS
- **JIT mode** (#95): `RTS_JIT=1 rts run` compila para memoria executavel, ~14x mais rapido
  que AOT no startup. Ambos paths via `&mut dyn Module`
- **Tail call optimization** (#93): recursao profunda sem stack overflow via `return_call`
- **First-class function pointers** (#97 fase 1): funcoes como valores, `call_indirect` no use
- **Intrinsics inline** (#87): `math.sqrt`, `math.random_f64`, etc. emitidos como IR direto
- **f64 modulo correto** (#89): via `fmod` libc
- **Jump table switch** (#91), **imm forms** (#94), **MemFlags::trusted** (#98),
  **stack_slot helper** (#99)
- **Capacidades TS**: template literals, ternario, bitwise ops, arrow/fn expressions,
  let/const scoping correto, compound assign (#48), `typeof`/`void`/`delete` (#51),
  `??` e optional call (#50), exponentiation (#52)

### Namespaces novos (13 total)
- **math** (#20) — 27 membros + 4 constantes (`MemberKind::Constant`)
- **bigfloat** — decimal fixed-point i128, pi com 29 digitos via Machin
- **time** (#14) — monotonic clock, wall clock, sleep
- **env** (#12) — vars, argv, cwd
- **path** (#13) — join/parent/normalize sem I/O
- **buffer** (#22) — Vec<u8> via HandleTable
- **string** (#25) — search/transform/replace/char ops
- **process** (#15) — exit/abort/pid/spawn/wait/kill
- **os** (#19) — platform/arch/home_dir/config_dir/cache_dir
- **collections** (#26) — HashMap<string, i64>, Vec<i64>

## Backlog priorizado

### Codegen
- **#97 fases 2/3** — arrow em qualquer posicao + captura imutavel/mutavel (depende #99 ✅)
- **#96 DWARF** — debug info no ObjectModule (investigacao complexa, ver comentario da issue)
- **#90 loop block params** — deferred; impl atual ja produz SSA correto via frontend
- **#92 autovec** — fechada como inviavel (Cranelift nao tem loop vectorizer)
- **#53 object literals**, **#54 array literals** — grande, desbloqueia classes, destructuring,
  `Array.prototype.*`
- **#62 try/catch/throw** — error handling idiomatico
- **#60/#61 for-of/for-in** — dependem de arrays/objects

### Namespaces pendentes
- **#23 fmt** — parse + format numerico (printf-like)
- **#21 hash** — SipHash (std::hash)
- **#24 crypto** — SHA/HMAC/AEAD (depende #22 ✅)
- **#28 regex** — pattern matching
- **#16 net** — TCP/UDP/HTTP (depende #22 ✅)
- **#17 thread**, **#18 channel**, **#27 sync** — concorrencia
- **#29-#39** — nicho (ffi, atomic, mem, ptr, num, hint, alloc, task, mpmc, simd, backtrace)

## Direcao alvo

Seguir a mesma ideia arquitetural da `main`, mas mantendo a API nova organizada.

Isso significa:
- pipeline completo com grafo de modulos, cache de objetos e link final;
- runtime support integrado ao `rts` via objetos `.o/.obj` precompilados;
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
- Consolidar runtime support via objetos `.o/.obj` precompilados no fluxo principal.
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
- `rts run` funciona fora do repo sem `Cargo.toml`, sem runtime archive `.lib/.a`.
- `io/fs/gc` estaveis no contrato novo da ABI.
- build e docs sem dependencia de `xtask`.
