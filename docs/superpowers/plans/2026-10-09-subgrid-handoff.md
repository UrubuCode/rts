# Subgrid (CSS Grid 2) — estado e como continuar

Escrito em 2026-10-09. Isto é a passagem de turno do trabalho de subgrid em `rts-dom`, para retomar depois.

## Objetivo

Passar 100% da pasta WPT `wpt-corpus/css/css-grid/subgrid/` sem nenhuma regressão nos testes de unidade.

**Regras que valem para cada mudança:**
- Todo `.rs` criado ou mexido fica com **≤ 500 linhas**.
- `cargo test -p rts-dom` a 100%.
- Nunca usar `cd` nos comandos (PowerShell, Windows).

## Como medir

```powershell
cargo test -p rts-dom
cargo build --release -p rts-dom --example claude-raster
bun scripts/wpt_reftests.mjs C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid            # número oficial
bun scripts/wpt_reftests.mjs C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid --filtro abs-pos-002   # iterar num teste
```

Os PNG das falhas ficam em `$env:TEMP\wpt-reftests\<nome>.teste.png` / `.ref.png`.
Para olhar um lado só: `target\release\examples\claude-raster.exe <html> <png>`.

## Estado atual (base boa)

| Métrica | Valor |
|---|---|
| Testes de unidade | 1394/1394 |
| WPT subgrid | **74/98** (75,5%) |
| WPT subgrid sem os 14 que têm `<script>` | 74/84 (88,1%) |

> [!WARNING]
> **Nada disto tem commit.** O trabalho todo de subgrid está no working tree (ver `git status`: `grid.rs`, `lines.rs`, `tracks.rs`, `subgrid.rs`, `subgrid_contrib.rs`, `abspos.rs`, `max_content.rs`, `subgrid_tests.rs`, ...). Faça commit numa branch antes de continuar. A raiz do repositório também tem muitos PNG/HTML/scripts de rascunho (`out*.png`, `test_edge_*.html`, `scripts/info_*.cjs`, `scripts/scratch_*.cjs`, ...) que **não** devem entrar no commit.

Tamanho dos arquivos principais: `grid.rs` 488, `lines.rs` 483, `subgrid.rs` 497, `subgrid_tests.rs` 497, `subgrid_contrib.rs` 476, `tracks.rs` 459. Quase todos estão perto do teto: qualquer código novo vai para um módulo novo.

## Onde paramos: `abs-pos-002`

### O problema

Um `position:absolute` filho de um grid/subgrid **posicionado** tem como containing block a **área de grade** das suas linhas (CSS Grid 1 §9.1 / Grid 2 §10.1), e não a padding box do container. Só as pontas `auto` usam a borda de padding. Hoje [`containing_block_rect`](../../../crates/rts-dom/src/layout/positioned/positioned.rs) devolve sempre a padding box.

Com a base boa, 6 das 10 grelhas do teste passam. As 4 que falham (1.565 px no total) usam a linha `1` ou `-1` num `.subgrid` com `padding: 4px 33px 1px 10px; border: 3px`:

| Grelha | Teste | Referência equivalente |
|---|---|---|
| 2 | `grid-column: 3 / -1` | `grid-column: 3; right: 33px` |
| 5 | `grid-column: auto / 1` | `auto / 1; right: -10px` |
| 7 (`.hr`, rtl) | `grid-column: 3 / -1` | `3 / -1; left: 13px` |
| 10 (`.hr`, rtl) | `grid-column: auto / 1` | `auto / 1; left: -33px` |

### O que foi tentado e revertido

Uma tentativa calculava o retângulo da área de grade em `grid/abspos.rs` (guardado num `thread_local` por `BoxId`) e `containing_block_rect` o consumia. Ela **piorou** o teste de 1.565 px para **9.790 px**, por isso voltou atrás. O código está guardado como referência:

- [`wip/2026-10-09-abspos-grid-area-cb.rs.txt`](wip/2026-10-09-abspos-grid-area-cb.rs.txt): `abspos.rs` com `col_line_x`/`row_line_y`, `record_out_of_flow`, `get_grid_cb`/`clear_grid_cb`. Também precisava de `Edge`/`resolve_edge` como `pub(in crate::layout)` em `lines.rs`, de `clear_grid_cb()` depois do laço out-of-flow em `layout.rs`, e de chamar `record_out_of_flow` depois de `total_w`/`total_h` em `grid.rs`.
- [`wip/2026-10-09-positioned-grid-cb.diff`](wip/2026-10-09-positioned-grid-cb.diff): o lado de `positioned.rs` (é preciso passar `target.box_id`). O diff ainda contém um `eprintln!` de debug, que não deve ir junto.

Resultado da tentativa, caixa prateada (x inicial..final) teste × referência:

| # | teste | ref | |
|---|---|---|---|
| 1 | 93..152 | 93..152 | ok |
| 2 | 93..236 | 93..239 | 3 px curto |
| 3 | 93..269 | 93..272 | 3 px curto (era ok na base) |
| 4 | 36..92 | 36..92 | ok |
| 5 | 36..45 | 36..45 | ok (era falha na base) |
| 6 rtl | 153..212 | 156..215 | 3 px deslocado (era ok) |
| 7 rtl | 46..212 | 229..272 | errado |
| 8 rtl | 36..212 | 36..215 | 3 px curto (era ok) |
| 9 rtl | 96..212 | 216..272 | errado (era ok) |
| 10 rtl | 96..236 | 240..272 | errado |

### O que as referências revelam (ler antes de tentar de novo)

Os números da referência mostram que, no browser, **a border box do `.subgrid` transborda a sua área de 240 px em 3 px** (vai de 33 a **276**, e não até 273): `padding-right 33 + border 3 = 36` não cabe na última trilha de 30 px. Com isso, no lado do teste:

- **LTR**: linha `1` = 46 (33 + 3 + 10), linha `-1` = **240**, borda de padding direita = **273**. O nosso motor faz a border box acabar em 273, por isso a linha `-1` sai a 237 e a borda de padding a 270. Daí os "3 px curto".
- **RTL**: as linhas contam a partir da direita, com a linha `1` em 273 e a primeira trilha com 27 px (linha `3` = 216, linha `5` = 156). A linha `-1` da grelha 7 resolve em **49**.

Então a causa raiz **não** está em `positioned.rs`. Está no **tamanho/posição da border box do subgrid quando o MBP de uma ponta é maior do que a trilha da ponta**: ver `build_axis` / `subgrid_mbp` em [`subgrid.rs`](../../../crates/rts-dom/src/layout/grid/subgrid.rs) (`track_ends` da última trilha = `phys_ends - smbp - embp`, que pode ficar abaixo do início da trilha) e o `total_w` em [`grid.rs`](../../../crates/rts-dom/src/layout/grid/grid.rs). Corrigir isto primeiro deve fazer as grelhas 2, 3, 6 e 8 baterem com a tentativa de CB. Depois falta acertar `col_line_x` em RTL para subgrid (`cs.reversed`) nas grelhas 7, 9 e 10.

**Próximo passo sugerido:**
1. Reproduzir só a grelha 3 (`grid-column:3`, sem CB de área) e fazer a border box do subgrid transbordar como no browser (33..276).
2. Reaplicar a tentativa guardada e conferir as 10 grelhas com o script de caixas prateadas.
3. Correr a pasta `css-grid` inteira (há testes `grid-abspos-*` fora de `subgrid/`) para garantir que a mudança de containing block não regride nada.

Script de rascunho que lista as caixas prateadas dos dois PNG: `C:\Users\nexga\.gemini\antigravity-cli\brain\b79960c8-34d8-462b-ba9b-43729b03753c\scratch\diff_check.mjs`.

## Outras falhas que faltam (10 sem `<script>`)

| Teste | Diff | Causa conhecida |
|---|---|---|
| `abs-pos-002` | 1.565 px | acima |
| `subgrid-item-with-margin-left-auto` | 390 px | o `<p>` tem texto diferente entre teste e ref no próprio upstream; provavelmente não dá para fechar |
| `item-percentage-height-001` | 800 px | 2,5 px de diferença na distribuição de espaço livre das linhas `auto` |
| `subgrid-baseline-001` / `-002`, `baseline-001` | 2,8k / 2,8k / 5,5k px | o grid não implementa `align-items`/`place-items: baseline` |
| `grid-gap-007` / `-008` / `-009` | 39,9k / 22,7k / 6,2k px | gap em subgrid aninhado/ortogonal |
| `orthogonal-writing-mode-001..006` | variam (o `-003` é o menor, ~1,3k px) | subgrid com writing-mode ortogonal |

Os outros 14 testes que falham têm `<script>` (o rasterizador corre sem JS) e ficam fora do alcance desta régua.

## Correções já feitas nesta frente (para contexto)

- `line-names-011`: `place_grid_items` recebe `explicit_cols_n = col_tracks.len()` (pode ser 0) em vez de `.max(1)`. Isto deixa um `subgrid` inválido (pai `<body>`) resolver nomes desconhecidos para a linha 2. De 3.074 px para 0.
