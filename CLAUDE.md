# CLAUDE.md — Indexador

**Este arquivo eh um INDEXADOR.** As regras vinculantes do projeto
vivem em `.claude/rules/<NN>-<topico>.md`. Ler este arquivo nao
substitui ler os arquivos de regras — ele apenas aponta para eles.

## REGRA #0 — META-REGRA OBRIGATORIA E ABSOLUTA

**Antes de iniciar QUALQUER tarefa, voce DEVE ler todos os arquivos
em `.claude/rules/` por inteiro e em ordem do prefixo numerico.
Cada regra eh vinculante.**

Esta eh a primeira e mais importante regra. Ela governa todas as
outras.

### Como aplicar

1. **Na primeira mensagem de cada sessao**, leia em ordem:
   - `.claude/rules/00-meta.md` (obrigatorio — meta-regras)
   - `.claude/rules/01-architecture.md`
   - `.claude/rules/02-runtime.md`
   - `.claude/rules/03-features.md`
   - `.claude/rules/04-workflow.md`
   - `.claude/rules/05-codegen-notes.md`

2. **Sempre que qualquer arquivo em `.claude/rules/` for
   modificado**, releia em ordem antes de tocar em codigo.

3. **Se uma regra entrar em conflito com uma instrucao do usuario**,
   peca confirmacao antes de violar a regra. Nao decida sozinho.

4. **Se uma regra estiver desatualizada** (codigo nao bate com o
   que esta escrito), atualize o arquivo correspondente no mesmo
   PR — nunca deixe regra mentirosa em vigor.

### Por que esta estrutura

- `CLAUDE.md` curto = indexador estavel, raramente muda
- `.claude/rules/<NN>-*.md` = regras agrupadas por tema, cada
  arquivo focado, mais facil de manter atualizado
- A regra #0 forca a IA a ler todos os arquivos — nao ha "escolher
  os relevantes" porque a meta-regra exige leitura completa

## Indice de regras (`.claude/rules/`)

| # | Arquivo | Conteudo |
|---|---|---|
| 00 | [00-meta.md](.claude/rules/00-meta.md) | **OBRIGATORIO** — REGRA #0 + RTK + local-rules.md + ZERO REGRESSAO |
| 01 | [01-architecture.md](.claude/rules/01-architecture.md) | Projeto + arquitetura `src/` + ABI + namespaces |
| 02 | [02-runtime.md](.claude/rules/02-runtime.md) | HandleTable + tokio + GC stack scanner + State + Sem codigo legacy |
| 03 | [03-features.md](.claude/rules/03-features.md) | Capacidades de linguagem + silent parallelism + async/Promise/Function |
| 04 | [04-workflow.md](.claude/rules/04-workflow.md) | Convencoes + progress bar + issues + criatividade ao testar + IR debug + benchmarks |
| 05 | [05-codegen-notes.md](.claude/rules/05-codegen-notes.md) | Otimizacoes + backlog + layout artefatos + specs |

## Lista das regras meta-vinculantes

Mantida em sincronia com `.claude/rules/00-meta.md`. Adicionar/
remover regras meta exige atualizar os dois lugares.

- **REGRA #0** (esta) — ler todos os arquivos em ordem
- **REGRA OBRIGATORIA: USO DO RTK** (`cat`/`head`/`tail`/`grep`/
  `find` viram `.github/rtk.exe ...`)
- **REQUISITO OBRIGATORIO: local-rules.md** (verificar e ler se
  existir)
- **REGRA OBRIGATORIA: ZERO REGRESSAO ANTES DE MERGE** (suite
  verde obrigatoria)

## Direcao de alto nivel

- `NEXT_STEPS.md` — proximas tarefas concretas
- `ROAD_MAP.md` — direcao estrategica
- `docs/specs/INDEX.md` — indice de specs tecnicas

## Quick reference

- Build: `cargo build --release`
- Test Rust: `cargo test --release --lib`
- Test TS: `target/release/rts.exe test`
- IR debug: `target/release/rts.exe ir file.ts 2>&1 | head -50`
- APIs: `target/release/rts.exe apis`
