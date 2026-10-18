# Lente: Réguas, testes e saúde do código

> Auditoria estrutural do motor HTML/CSS/DOM, 2026-09-04. Relatório de UM agente
> (Sonnet, só leitura, obrigado a citar `ficheiro:linha` e a verificar no código e não
> nos docs), tal como o devolveu — sem edição de conteúdo. A síntese e o veredito
> global estão no [README](README.md) desta pasta. O estado de referência é `main`
> em `fc84d04f` (2026-09-03); as linhas citadas são desse commit.

**Veredito desta lente:** `certa-com-divida`

## Resumo

Nesta lente, a resposta é: a ESTRUTURA das réguas é a certa — corpus medido no Chrome com regra anti-fraude explícita ("fixture que falha fica a falhar"), denominador verificado em vez de assumido, e um refactor real (layout.rs 9987→módulos) provam que a equipa entende o que uma régua de motor de browser precisa de ser e já a construiu nesse formato uma vez. O problema não é o desenho do instrumento, é a sua OPERACIONALIZAÇÃO: nenhuma dessas réguas — nem os 724 testes unitários do rts-dom, nem as 49 fixtures CSS, nem o dom_metrics — corre em CI; toda a proteção depende de alguém lembrar-se de correr o comando localmente, exatamente a classe de falha silenciosa que o CLAUDE.md já documenta para o cross-runtime, mas aqui sem sequer o badge que não bloqueia. Falta por inteiro uma régua de PINTURA (screenshot vs Chrome) — a única verificação de layout é geométrica/computada, nunca a imagem — e a régua de interação automatizada existe mas só ao nível de API DOM (dispatch, listeners, teclado), não de framework inteiro rodando numa janela real (esses ficam a demos manuais lidas por humano). Por fim, a disciplina de tamanho de ficheiro e de doc-código que o próprio CLAUDE.md prescreve não está a ser seguida nos ficheiros mais ativos do crate (dom.ts, bloco.rs) nem no seu próprio mapa de crates, que já está desatualizado antes mesmo desta auditoria. Nada disto exige desfazer arquitetura; exige ligar o que já existe ao portão de CI e manter a disciplina que o texto já pede.

## O que está bem (com evidência)

- **O corpus de fixtures CSS (tests/css/) é medido no Chrome real e tem uma regra explícita anti-fraude: uma fixture que falha FICA a falhar (não se apaga nem se ajusta o esperado), e o denominador conta os .html que existem, marcando 'SEM ESPERADO' em vez de omitir silenciosamente.**
  - Evidência: tests/css/README.md linhas ~30-55 e 'O denominador' (fim do ficheiro); confirmado hoje: 49 .html e 49 .esperado.json em tests/css/ (contagem via `ls | wc -l`), condizente com o método descrito.
- **O motor abandonou de facto a reescrita textual de scripts de página por uma resolução de escopo feita pelo próprio compilador — mudança estrutural real, não cosmética, e documentada com o motivo e o que foi medido.**
  - Evidência: crates/rts-codegen/src/emit/page.rs:1-135 (`emit_page_program`), comentário 'What this replaces, and why the replacement is smaller' cita a técnica antiga (varredura léxica reescrevendo `__G.<name>`) como abandonada; docs/ui/page-script-bridge.md (30/08) descreve as cinco peças atuais, incluindo o novo crate `rts-dom-bridge`.
- **A medição de texto (TextMeasurer) tem UMA implementação, não duas: rts-dom define o trait, rts-egui implementa-o, e o código regista explicitamente que a alternativa de duplicar a escolha de fonte na pintura foi REJEITADA.**
  - Evidência: crates/rts-dom/src/layout/medida.rs (trait); crates/rts-egui/src/frame/render/medida.rs:9-13, comentário: 'a alternativa rejeitada foi deixar a pintura com a sua cópia da escolha'.
- **Existe uma classe de testes automatizados de interação DOM/eventos que corre em CI via `rts test`, não só demos visuais manuais — cobrindo listener options, teclado, timers de página, escopo partilhado entre scripts, e falhas reais encontradas ao correr Preact real.**
  - Evidência: tests/claude-dom-keyboard-events.test.ts, claude-dom-listener-options.test.ts, claude-dom-page-timers.test.ts, claude-dom-escopo-compartilhado.test.ts, claude-dom-script-globals.test.ts, tests/claude-preact-precisa-destas-tres.test.ts (3 `expect()` citando o código-fonte exato do Preact 10.24.3 que cada uma prova); `.github/workflows/build-artifacts.yml:205` corre `target/release/rts test`.
- **Houve um refactor estrutural real do layout, não só acumulação de features: um ficheiro layout.rs de 9 987 linhas foi partido em módulos coesos (bloco.rs, vertical.rs, flex.rs, grid.rs, quebra.rs, linha.rs, etc.) num único commit provado.**
  - Evidência: git log -L confirma `crates/rts-dom/src/layout/quebra.rs` e `linha.rs` como `new file` no commit 21/08 'o núcleo sai — layout.rs 9 987 → 307, e um portão único prova 4 316 linhas movidas'.

## Findings

### 1. [dívida] Nenhum instrumento de medição do rts-dom corre em CI — nem os 724 testes unitários Rust, nem o corpus de 49 fixtures CSS medidas no Chrome, nem o dom_metrics. Tudo depende de disciplina manual (o próprio CLAUDE.md, secção MANDATORY).

- **Evidência:** `grep -n "cargo test" .github/workflows/*.yml` = zero resultados nos 4 workflows (benchmarks.yml, build-artifacts.yml, cross-runtime.yml, node-suite.yml); `grep -n "css_fixtures|dom_metrics|examples/claude-css" .github/workflows/*.yml` = zero resultados. build-artifacts.yml só corre `target/release/rts test` (o suite `*.test.ts`, não os testes Rust do crate) e três smoke tests de compile/run.
- **O que um browser faz:** Blink e WebKit correm layout-tests, web-platform-tests e unit tests do próprio motor em bots de CI a cada patch, bloqueantes por default (com listas de expectativa explícitas para falhas conhecidas) — a régua faz parte do gate, não é um script que alguém tem de lembrar de correr.
- **Recomendação:** Acrescentar um job de CI (mesmo que não-bloqueante inicialmente, como os outros três já admitidamente são) que corra `cargo test --profile fast -p rts-dom` e `scripts/css_fixtures.sh`, escrevendo o resultado num badge/relatório do mesmo jeito que `cross-runtime` já faz — para que uma regressão apareça num check e não só quando alguém lembra de correr localmente.

### 2. [dívida] Não existe nenhuma régua de PINTURA (comparação de pixels/screenshot contra Chrome). O corpus de 49 fixtures compara apenas getBoundingClientRect/getComputedStyle — geometria e propriedades computadas, nunca a imagem renderizada.

- **Evidência:** tests/css/README.md descreve o método de medição inteiramente em termos de `getBoundingClientRect()`/`getComputedStyle()`; não há ficheiro .png/.esperado.png no corpus; `find . -iname '*screenshot*'` não encontrou nenhum script, só menções textuais em docs (arquitetura.md, roadmap.md); o único jeito de ver a pintura real (cores, gradientes, texto, anti-aliasing) hoje é abrir a janela manualmente (`examples/claude-react-vida.ts`, que corre um loop indefinido pintando e só imprime texto final no console).
- **O que um browser faz:** Blink mantém layout tests (geometria/texto, o equivalente do que existe aqui) SEPARADOS de pixel tests (screenshot diff), porque cada um apanha uma classe diferente de regressão — gradiente na direção errada, sombra cortada, hinting de fonte errado nunca aparecem num rect certo.
- **Recomendação:** Um harness mínimo de screenshot-diff (mesmo que sem tolerância perceptual sofisticada — diff de bytes com tolerância por pixel) sobre um subconjunto pequeno das 49 fixtures fecharia a lacuna mais barata: pintura de cor sólida, gradiente e borda já têm fixture geométrica, falta só capturar a imagem em vez do rect.

### 3. [cosmético] O próprio corpus de fixtures tem dois números conflitantes e ambos desatualizados: tests/css/README.md diz '42 fixtures, 7 passam' (medido 18/08); docs/ui/css-implementation-gaps.md diz '49 fixtures, 41 passam' (medido 27/08) — mesma régua, 9 dias de intervalo, e nenhum dos dois foi re-medido nos 8 dias seguintes até hoje (04/09).

- **Evidência:** tests/css/README.md, secção 'O número, hoje': '2026-08-18: 7 das 42 fixtures passam'. docs/ui/css-implementation-gaps.md linha 3: 'O resultado actual é 41/49 fixtures aprovadas'. Contagem real hoje: `ls tests/css/*.html | wc -l` = 49, `ls tests/css/*.esperado.json | wc -l` = 49 — confirma que o README.md da própria pasta das fixtures está a citar um denominador (42) que já não existe.
- **O que um browser faz:** não aplicável diretamente — é uma questão de disciplina de manutenção do próprio repositório, mas o CLAUDE.md já resolveu o mesmo problema para o número cross-runtime gerando-o automaticamente entre marcadores CROSS_RUNTIME_STATS em vez de o escrever à mão.
- **Recomendação:** Aplicar ao tests/css/README.md o mesmo padrão já adotado para o número cross-runtime: gerar o bloco de estatísticas automaticamente a partir da última corrida de scripts/css_fixtures.sh, com marcadores, em vez de editar o número à mão.

### 4. [dívida] dom.ts (1847 linhas) e layout/bloco.rs (1061 linhas), mais 7 outros ficheiros do rts-dom, excedem o teto de 500 linhas do CLAUDE.md — e não são debito congelado: dom.ts cresceu de forma quase inteiramente aditiva (2086 linhas adicionadas, 239 removidas em toda a sua história) e 16 dos últimos 60 commits que tocam rts-dom/docs-ui (27%) mexem diretamente em bloco.rs ou dom.ts.

- **Evidência:** `find crates/rts-dom -name '*.rs' -o -name '*.ts' | xargs wc -l | sort -rn` lista dom.ts=1847, syntax.rs=1122, bloco.rs=1061, scenarios.rs=829, parse/mod.rs=724, sheet.rs=580, vertical.rs=556, mutacao.rs=522, fragmento.rs=509 (9 ficheiros acima de 500 linhas em 193 do crate); `git log --numstat -- crates/rts-dom/src/dom.ts` soma 2086 adições / 239 remoções desde a criação; contagem manual sobre os hashes dos últimos 60 commits mostra 16/60 tocando `layout/bloco.rs` ou `src/dom.ts` via diff (não só menção na mensagem).
- **O que um browser faz:** não é uma questão de arquitetura de browser per se, mas de disciplina de tamanho de ficheiro que o próprio repositório se impôs; a analogia útil é que Blink evita 'god files' (LayoutObject.cpp já foi dividido várias vezes) precisamente porque um ficheiro que cresce sem parar vira gargalo de revisão e de merge.
- **Recomendação:** Aplicar a regra que o próprio CLAUDE.md já escreve ('split into a folder of cohesive modules... new code lands in a small focused module, never appended to something already oversized') ao dom.ts e ao bloco.rs na próxima vez que alguém os editar por feature, em vez de continuar a acrescentar.

### 5. [cosmético] Há código morto deixado pelo próprio refactor que partiu layout.rs (9 987→vários ficheiros, commit de 21/08): `layout_inline_line` (linha.rs), `wrap_text` (quebra.rs) e `fragment_count` (dom/caches.rs) não são chamados por ninguém, ainda duas semanas depois.

- **Evidência:** `cargo check -p rts-dom` (corrido uma vez, per instrução) devolve 12 warnings, incluindo 'function `layout_inline_line` is never used' (layout/linha.rs:12) e 'function `wrap_text` is never used' (layout/quebra.rs:425) e 'method `fragment_count` is never used' (dom/caches.rs:90); `git log -L` confirma que ambas as funções nasceram no mesmo commit de partição do layout.rs (21/08), i.e. não foram deixadas mortas de propósito por outra razão registada.
- **O que um browser faz:** não aplicável — é uma violação direta de uma regra própria do repositório.
- **Recomendação:** Apagar as três funções (CLAUDE.md, secção Conventions: 'No dead code. Deleted in the change that stopped reaching it').

### 6. [cosmético] O crate `rts-dom-bridge` (2228 linhas, 9 ficheiros — inclui `events.rs` com 542 linhas, também acima do teto) não consta do 'Repository map' do CLAUDE.md, que se afirma como 'Fifteen crates' mas na verdade lista dezasseis e o repositório tem hoje dezoito (falta também `rts-physics`, 2524 linhas). `rts-dom-bridge` é descrito por outro documento (08-30, cinco dias antes desta auditoria) como parte de cinco peças SEM AS QUAIS um `<script>` de página não corre.

- **Evidência:** `ls crates | wc -l` = 18 hoje; CLAUDE.md, secção 'Repository map': texto diz 'Fifteen crates' mas o bloco de código lista rts-cranelift/codegen/core/host/macro/std/node/ui/runtime/egui/dom/render/input/linker/cli/napi = 16 nomes, nenhum deles `rts-dom-bridge` nem `rts-physics`; docs/ui/page-script-bridge.md, tabela 'As cinco peças', linha 1: `DomScope` em `rts-dom-bridge/src/scope.rs`; `wc -l` sobre `crates/rts-dom-bridge/src/*.rs` soma 2228, com `events.rs`=542.
- **O que um browser faz:** não aplicável a estrutura de browser; é sobre a própria doc de entrada do repositório estar a contradizer o código que ela existe para mapear — o que o próprio CLAUDE.md identifica, na sua introdução, como o problema recorrente que o motivou a existir.
- **Recomendação:** Atualizar o bloco 'Repository map' para incluir `rts-dom-bridge` e `rts-physics`, e corrigir a contagem ('Fifteen' → o número real).

## O que o agente NÃO verificou (e diz que não verificou)

- Não foi corrida a suite completa (`target/release/rts test` nem `cargo test -p rts-dom`), só `cargo check -p rts-dom` uma vez, por restrição de tempo/regra da tarefa — os números '724 testes' e '12 warnings' são de uma leitura estática/uma corrida de check, não de uma corrida completa da suite.
- Não foi verificado se scripts/parity/ (o dump de 16 813 elementos mencionado no prompt) ainda corre ou está atualizado — só se confirmou que o diretório e os scripts (compare.mjs, regua.mjs, chrome_extract.mjs) existem.
- Não foi lido docs/ui/dom-metrics.md além do índice e do início — não se verificou se as otimizações e números lá descritos (campanha até 08-18) foram re-medidos depois de commits recentes ao layout.
- Não foi verificado se os 6 invariantes do roadmap além do 4º e 5º (que se confirmaram parcialmente obsoletos/violados: Rust hoje casa strings CSS diretamente em inherit_kw.rs/initial.rs, e dom.ts/window.ts já usam `class`, não só função-plana) continuam válidos no código — só 2 dos 6 foram checados com evidência de ficheiro:linha.
