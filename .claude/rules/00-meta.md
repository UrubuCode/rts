# Regras meta — leitura obrigatoria

Este arquivo eh o primeiro a ser lido. Define como tratar o resto do
sistema de regras. Apos ler este, leia os demais em ordem (`01-` ate
`05-`) — cada um eh vinculante na mesma medida.

## REGRA #0 — META-REGRA OBRIGATORIA E ABSOLUTA

**Antes de iniciar QUALQUER tarefa, voce DEVE ler todos os arquivos
em `.claude/rules/` por inteiro e seguir TODAS as regras que eles
definem, sem excecao, sem omissao, sem "escolher as importantes".
Cada regra eh vinculante.**

### Como aplicar

1. Na primeira mensagem de cada sessao (e sempre que qualquer arquivo
   em `.claude/rules/` for modificado), leia os arquivos em ordem
   crescente do prefixo numerico antes de tocar em codigo.
2. Cada secao marcada `## REGRA OBRIGATORIA:` eh vinculante mesmo
   quando o contexto da tarefa parece nao exigir.
3. Cada secao `## Convencoes`, `## Regras`, `## ABI ...`,
   `## Estrutura ...` define convencoes que devem ser respeitadas em
   qualquer mudanca de codigo.
4. Se uma regra entrar em conflito com uma instrucao do usuario, peca
   confirmacao antes de violar a regra. Nao decida sozinho.
5. Se uma regra estiver desatualizada (o codigo nao bate mais com o
   que esta escrito), atualize o arquivo correspondente no mesmo PR —
   nunca deixe regra mentirosa em vigor.

### Mapa de leitura

Ler estes arquivos em ordem (caminho relativo a raiz do repo):

| Arquivo | Conteudo |
|---|---|
| `.claude/rules/00-meta.md` | Este arquivo — meta + RTK + local-rules + zero regressao |
| `.claude/rules/01-architecture.md` | Projeto + Arquitetura + ABI + Namespaces |
| `.claude/rules/02-runtime.md` | HandleTable + tokio + GC + State |
| `.claude/rules/03-features.md` | Silent parallelism + async/Promise/Function + capacidades |
| `.claude/rules/04-workflow.md` | Convencoes + progress bar + issues + testes + benchmarks |
| `.claude/rules/05-codegen-notes.md` | Otimizacoes + backlog + layout artefatos |

### Regras meta-vinculantes (lista canonica)

- **REGRA #0** (esta) — ler todos os arquivos em ordem
- **REGRA OBRIGATORIA: USO DO RTK** (abaixo)
- **REQUISITO OBRIGATORIO: local-rules.md** (abaixo)
- **REGRA OBRIGATORIA: ZERO REGRESSAO ANTES DE MERGE** (abaixo)

Adicionar/remover regras meta exige atualizar esta lista no mesmo
commit.

## REGRA OBRIGATORIA: USO DO RTK PARA COMANDOS ESPECIFICOS

### Comandos que DEVEM usar `.github/rtk.exe <execution>`:

| Comando | Substituicao Obrigatoria |
|---------|-------------------------|
| `cat`   | `.github/rtk.exe cat`    |
| `head`  | `.github/rtk.exe head`   |
| `tail`  | `.github/rtk.exe tail`   |
| `grep`  | `.github/rtk.exe grep`   |
| `find`  | `.github/rtk.exe find`   |

Nao use os comandos nativos diretamente (ex: `cat file.txt` → use
`.github/rtk.exe cat file.txt`).

### Excecoes e Observacoes

- **Ferramentas nativas do Claude Code** (Read, Grep, Glob) NAO
  passam pelo hook e NAO sao reescritas automaticamente.
- Para obter a saida compacta do RTK nesses fluxos de trabalho, use
  comandos shell com RTK ou chame diretamente:
  - `.github/rtk.exe read` (em vez de Read tool)
  - `.github/rtk.exe grep` (em vez de Grep tool)
  - `.github/rtk.exe find` (em vez de Glob tool)

## REQUISITO OBRIGATORIO: local-rules.md

Antes de iniciar qualquer tarefa, voce **DEVE** verificar se existe
um arquivo `local-rules.md` na raiz do projeto.

**Se existir, ler eh obrigatorio** — nao eh opcional, nao pular, nao
assumir conteudo, nao prosseguir sem ler. Se nao existir, prossiga
normalmente.

Quando existir, trate seu conteudo como regras adicionais definidas
pelo desenvolvedor que esta trabalhando nesta copia local. Essas
regras tem prioridade sobre preferencias genericas e devem ser
respeitadas durante toda a sessao.

O arquivo `local-rules.md` eh pessoal de cada desenvolvedor e **nao
deve ser versionado** (ja esta no `.gitignore`).

## REGRA OBRIGATORIA: ZERO REGRESSAO ANTES DE MERGE

**Toda PR — sem excecao — so pode ser merged depois de validar que
TODOS os testes da suite atual ainda passam, junto com os testes
novos da feature/fix.**

Suite minima a rodar antes de aprovar merge:

```bash
cargo build --release             # build limpo (zero warnings de erro)
cargo test --release --lib        # 100% dos testes unit + integration verdes
```

Se o PR mexe em codigo de runtime/codegen/GC, tambem:

```bash
target/release/rts.exe test       # suite TS via rts:test
```

### Regras praticas

- **Build quebrado bloqueia merge.** Mesmo que "so warning".
  Investigar antes.
- **1 teste falhando bloqueia merge.** Nao importa se "nao tem
  relacao com o PR". Falha eh falha.
- **Nao ha excecao de "consertar depois".** Se a feature exige
  refator que quebra teste, refatore + corrija o teste **no mesmo
  PR**, com justificativa explicita no commit.
- **Fixtures de codegen (`tests/fixtures/*.ts/.out`) sao parte da
  suite.** Se mudou comportamento esperado, atualizar `.out` e
  justificar.
- **PRs grandes que tocam varias areas devem rodar a suite
  incrementalmente** durante o desenvolvimento, nao so no fim. Se
  quebrou no meio, parar e corrigir antes de avancar.

### Por que essa regra existe

Em projeto com 2 devs + IA acelerando velocidade, tentacao de
"mergear e arrumar depois" mata o projeto em 30 dias. Cada regressao
silenciosa acumula ate a suite virar mentira (testes verdes mas
codigo quebrado em casos nao cobertos). Manter zero regressao eh o
que separa projeto que cresce em qualidade do que apodrece em
features.

Disciplina aqui eh inegociavel. Se IA propoe solucao que quebra
suite, IA esta errada — independente de quao convincente o
argumento. Forcar outra abordagem.
