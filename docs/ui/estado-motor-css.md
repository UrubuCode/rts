# O motor de CSS e layout — o que foi feito, o que falta

Estado em **2026-08-18**, no fim da campanha de paridade com o Chrome.
Escrito para quem retomar isto sem ter estado presente.

Os números têm todos a mesma proveniência salvo indicação: `bash
scripts/parity/run.sh` sobre `pagina.html` + `pagina.css` (a Wikipédia
pt/Brasil, 2 MB de HTML e 257 KB de CSS), viewport 1280x800, JavaScript da
página desligado, contra um Chrome real.

---

## Onde estávamos e onde estamos

| | início | fim |
|---|---|---|
| elementos sem caixa nenhuma | 14 173 de 16 813 (84%) | **0** |
| erro mediano em `x` | 142 px | **19,6 px** |
| excesso de altura nas FOLHAS | 24 545 px em 3 032 `<a>` | 12 541 px |
| folhas certas ao pixel | — | **4 752** |
| fixtures do corpus a passar | 7 de 42 | **25 de 42** |
| testes do `rts-dom` | 232 | **376** |
| propriedades CSS reconhecidas | 68 de 363 usadas | ver `css-support.md` |

E a página real **aparece na janela**: `scripts/captura/out/` tem o antes
(branco), o meio (com blocos cinzentos no lugar dos ícones) e o depois.

---

## O que foi implementado

**Propriedades**: o shorthand `background` (era ignorado por inteiro),
`border-*` por lado, `text-decoration` shorthand, `vertical-align`, `clear`,
`visibility`, `list-style-*`, `border-collapse`, `border-spacing`,
`table-layout`, `mask-image` (reconhecido; ver limitações), e o lote de texto.

**Layout**: `display:list-item` com marcadores e contadores; tabelas (largura de
coluna por conteúdo, altura de linha, `colspan`/`rowspan`, linhas e células
anónimas, `<caption>`); `grid-template-areas` e `grid-area`; `minmax()` como
faixa e não como máximo; geometria para elementos inline; `<br>`; caixas
atómicas para `<img>`/`<video>`/`<iframe>` inline.

**Seletores**: `~`, `+`, `[attr]` e variantes, `:where` (especificidade zero) e
`:is`.

**Parser**: `<html>`/`<body>` implícitos, e `innerHTML` como fragmento.

---

## O que falta, por ordem de valor

**1. A largura dos itens de flex nos menus.** É o que sobra maior e está
localizado: dos 681 `<a>` folha com excesso de altura, 551 têm a LARGURA
também errada e somam 6 471 px dos 7 150. Com a largura errada o texto quebra
noutro sítio e a caixa fica alta — a altura deles não é o defeito.

Atenção a uma armadilha já paga: `flex-grow`/`shrink`/`basis` **estão
implementados e corretos** — foram medidos em isolamento, incluindo o shrink
ponderado pela base. Um diagnóstico anterior atribuiu-lhes a culpa a partir do
sintoma certo com a causa errada.

**2. A altura acumulada (`y`).** É o eixo pior: mediana de milhares de pixels,
enquanto o `x` está em 19,6. As caixas individuais estão certas (`|dh|` mediano
3,8 px) — o que erra é a SOMA. Duas varreduras feitas com sonda dedicada dizem
que **a largura do texto é ~5x mais influente que a altura de linha** para a
altura do documento: um balanço de 43% na largura move 20% da altura; um de 60%
na altura de linha move 3,7%.

**3. Tabelas**: `th` 1 581 px e `td` 812 px de excesso, mais a
`section[13]/div[3]/table[1]` com +5 467 px — a maior concentração única.

**4. `::before`/`::after` com `content`** — 117 usos na folha do MediaWiki, não
implementado.

**5. `mask-image` a sério.** Hoje é reconhecido e o fundo é SUPRIMIDO, para não
pintarmos um quadrado onde o browser desenha um glifo. Quando houver máscaras, o
fundo volta a ser pintado e recortado por elas.

**6. As 17 fixtures que ainda falham** (`bash scripts/css_fixtures.sh`), com o
esperado medido num Chrome real — cada uma isola um mecanismo.

---

## As armadilhas que já custaram tempo

Estão em `docs/ui/parity-chrome.md` com detalhe. Em resumo:

- **O número pertence ao BINÁRIO, não ao `HEAD`.** Uma medição com um executável
  de há duas horas não credita o trabalho que entrou entretanto.
- **A percentagem pode não mexer enquanto tudo melhora.** 12,4% com erro mediano
  de 3 114 px e 12,4% com 547 px são estados muito diferentes.
- **Verificar a ENTRADA.** Uma corrida respondeu "2 de 2 casam (100%)" — sinal de
  desastre, não de sucesso: o denominador tinha encolhido em silêncio.
- **Uma medição feita sobre um working tree partilhado a meio de edições não
  vale.** Um relato de colapso do artigo (13 940 → 7 884 elementos) não se
  reproduziu no estado commitado: 13 819 elementos com altura, 51 de 51
  `<section>` com altura.
- **Duas respostas para a mesma pergunta** foi a causa (não o sintoma) quatro
  vezes: o `line-height`, a percentagem no tamanho intrínseco, a medida do
  `<input>`, e a altura da linha do inline.

---

## Como retomar

```bash
bash scripts/parity/run.sh                      # a régua contra o Chrome
bash scripts/css_fixtures.sh                    # as 42 fixtures
bash scripts/captura/janela.sh <ts> <png> '*t*' # ver na tela
cargo test -p rts-dom                           # 376 testes
```

`OUT=` no `run.sh` para não escrever por cima da medição de referência.
