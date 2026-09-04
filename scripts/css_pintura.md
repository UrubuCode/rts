# A régua de pintura

Compara PIXEL, não caixa nem estilo computado. As outras réguas de
`tests/css/` (`css_fixtures.sh`, `css_fixtures_medir_edge.mjs`) respondem
"a caixa está no sítio certo?" e "a propriedade computada é a mesma?"; esta
responde "o desenho é o mesmo?" — a que faltava segundo o achado 2 de
`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/06-reguas-e-saude-do-codigo.md`.

## As três peças

1. **`cargo run -p rts-dom --example claude-raster -- <fixture>.html <saida>.png`**
   Faz layout da fixture a 1280×800 com o `ApproxMeasurer` (o mesmo medidor
   do `claude-paint-dump.rs` — trocar de medidor trocaria o layout que se
   está a rasterizar) e rasteriza a `DisplayList` para um PNG RGBA, sem egui
   nem wgpu. Grava também `<saida>.png.mask.json`: os retângulos de texto que
   NÃO pintou.

2. **`bun scripts/css_fixtures_screenshot_edge.mjs [nomes...]`**
   Abre cada fixture DIRETAMENTE (não num iframe) a 1280×800 no Edge headless
   por CDP e grava `tests/css/pintura/<nome>.blink.png` via
   `Page.captureScreenshot`. Precisa de `bun scripts/css_fixtures_serve.ts`
   a correr (porta 8731) — a mesma dependência da régua de N.

3. **`bun scripts/css_pintura_comparar.mjs [nomes...]`**
   Lê `<nome>.rts.png` e `<nome>.blink.png` de `tests/css/pintura/`,
   decodifica os dois (um decodificador PNG mínimo — `node:zlib` para o
   `inflate`, "unfilter" dos 5 filtros à mão) e responde por fixture:
   `% diferente` (pixels acima da tolerância por canal, 8/255 por omissão,
   `TOLERANCIA=N` muda) e `% de área ignorada` (a máscara de texto do passo
   1). Grava `<nome>.diff.png` — vermelho onde diverge, cinza onde foi
   ignorado, a cor original onde bate.

## O que é ignorado, e porquê

**Texto**, sempre. Nenhum dos dois lados desta régua usa fonte real do nosso
motor: o `ApproxMeasurer` mede `0.5×size` por caractere, sem glifo nenhum —
comparar essa área contra um `Page.captureScreenshot` do Blink (Segoe UI de
verdade) mediria o erro do medidor aproximado, não um defeito de pintura. O
`claude-raster.rs` por isso NÃO pinta texto — decisão tomada no rasterizador,
não escondida no comparador — e grava onde ficaria como máscara.

**Imagem** (`<img>`, `background-image` com URL), por agora — e desde o lote
U-pintura-2 a área dela vai para a MÁSCARA como a do texto, e conta na "área
ignorada": antes contava como diferença (`claude-object-fit` dava 1,95 % só
disso, que era o exemplo a não ter o que o motor tem). `DisplayItem::Image`
aponta para um handle do `HandleTable`, que o exemplo não tem (não instancia
`Engine`/`Registry`, só `Dom`+layout — trazê-los para um exemplo de
rasterização é o preço de pintar UMA fixture no corpus atual). Nenhuma
fixture de `tests/css/` depende disto para o que fixa; quando uma depender,
o custo de instanciar o handle table paga-se então.

**Bordas com junção diagonal** pintam-se (`DisplayItem::Quad`, lote
U-pintura-2): o triângulo de CSS passou de 1,58 % para 0,04 %.

**Cantos arredondados**, aproximados a quadrados. Rasterizar um arco exige
mais do que um "fill rect", e a tolerância por canal (8/255) já absorve a
faixa de poucos pixels onde um canto reto diverge de um arredondado — não
valia o código para o que a régua está a verificar primeiro.

**Sombra sem desfoque** (`box-shadow` pinta achatada, sem blur gaussiano) —
mesma razão: código a mais para o que esta primeira entrega precisa provar.

## Como ler o número

`% diferente` é sobre os pixels COMPARADOS (área total menos a área
ignorada), nunca sobre o total — a mesma regra do "verifique a entrada" do
`CLAUDE.md`: se a área ignorada crescer sem que ninguém note, o "% diferente"
cai por ter menos para comparar, e pareceria uma melhoria que não houve. É
por isso que o comparador imprime as duas percentagens sempre juntas.

Um valor de referência foi medido antes de qualquer PR se apoiar nesta régua
(`tests/css/claude-cor-e-fundo.html`, só cor sólida e fundo — sem gradiente
nem texto): **0% diferente, 0.09% de área ignorada** (um `div` vazio ainda
gera um item de texto de largura zero). E com gradiente linear
(`tests/css/claude-background-camadas.html`, `linear-gradient(red, blue)`):
**0.24% diferente, 0% de área ignorada**.

## Tamanho

O `claude-raster.rs` escreve PNG sem compressão (bloco `deflate` "stored") —
um PNG de 1280×800 RGBA sai por volta de **4 MB**, contra ~5-6 KB do
`Page.captureScreenshot` do Blink (que comprime de verdade). Por isso
`tests/css/pintura/` está no `.gitignore`: os PNG são regeneráveis a
qualquer momento pelos três comandos acima, e não há decisão tomada aqui
sobre quais — se algum — entram como esperado versionado; essa decisão fica
para quando uma fixture específica precisar de um esperado de pintura fixo.

## O número, hoje

**2026-09-04, primeira medição sobre o corpus inteiro (86 fixtures, Edge 152
headless):** 79 fixtures com ≤ 0,5 % de pixels diferentes, 82 com ≤ 2 %, e
**4 acima de 2 %** — `claude-transform-origin` (6,06 %) e
`claude-transform-nao-afeta-fluxo` (2,05 %): a rotação é pintada como caixa
axis-aligned (o backend não roda); `claude-overflow` (5,57 %): o recorte por
`overflow` não é aplicado pelo rasterizador; `claude-sel-target` (2,55 %).
Texto ignorado em todas (a máscara): no máximo 1,4 % da área. Estes quatro
são os primeiros alvos de um lote de pintura, e o número que os fecha é este.

**2026-09-04, lote U-pintura-1 (`feat/dom-lote-pintura-rotacao-clip`) — os
quatro fechados, medidos de novo sobre o corpus inteiro:**
`claude-transform-origin` 6,06 % → **0 %**, `claude-transform-nao-afeta-fluxo`
2,05 % → **0,38 %**, `claude-transform-skew-matrix` 1,47 % → **0,08 %**,
`claude-overflow` 5,57 % → **0 %**, `claude-sel-target` 2,55 % → **0,05 %**
(a régua ganhou este quinto: estava medido a 2,55 %, não citado na lista de 4,
porque a tabela anterior arredondava "82 com ≤ 2 %" — ele já estava fora
dela). Duas causas, não uma: a matriz de `transform` (`Mat2d`) agora viaja na
`DisplayList` como `PushTransform`/`PopTransform` em vez de mutar o `rect` por
aproximação (norma das colunas), e o rasterizador/backend pintam o
quadrilátero real; e o `BeginClip` do `overflow` tinha o `filhos_antes` errado
— contava `list.children.len()` DEPOIS de o filho já ter sido anexado, então
`itens.rs::walk_items` desenhava sempre o filho ANTES de entrar no clip, e o
recorte nunca continha nada (não era uma questão de eixo aberto — a correção
inicial de "um eixo `visible` sozinho não recorta" não mudou o número; o que
mudou foi este índice). Nenhuma das 86 piorou: o resto do corpus continua
como estava (dois já estavam entre 0,5 % e 2 % antes deste lote e continuam —
`claude-object-fit` 1,95 %, `claude-triangulo-de-borda` 1,58 % — sem relação
com transform/overflow). `crates/rts-dom/PLAN.md` §0, linha `U-pintura-1`.

**2026-09-04, depois do lote U-pintura-1 (rotação na pintura, recorte por
`overflow`, `fixar-hash` no rasterizador):** 84 fixtures com ≤ 0,5 %, 86 com
≤ 2 %, **nenhuma acima de 2 %**. As duas maiores que ficam: `claude-object-fit`
(1,95 %) e `claude-triangulo-de-borda` (1,58 %).

**2026-09-04, lidas as duas:** `claude-triangulo-de-borda` (1,58 %) é a
JUNÇÃO das bordas — o Blink pinta cada lado como um trapézio até ao canto
interior e nós pintamos rectângulos por cima dos vizinhos, o que aparece nas
áreas transparentes do triângulo; `claude-border-juncao` fixa esse mecanismo
sozinho (0,13 % antes do código). `claude-object-fit` (1,95 %) é a área das
quatro `<img>` que o rasterizador não pinta por decisão (sem handle table):
a correção é do INSTRUMENTO — mascarar `DisplayItem::Image` como se mascara
texto, e reportar a área — não do motor.

**2026-09-04, lote U-pintura-2:** 87 das 90 fixtures com ≤ 0,5 %, 89 ≤ 2 %.
`claude-triangulo-de-borda` 0,04 %, `claude-border-juncao` 0,02 %. Ficam
`claude-font-unidades-ch-ex` 2,45 % (lote T) e `claude-object-fit` 1,95 % —
este último já NÃO é o instrumento: o rasterizador pinta 0 itens nessa
fixture, porque o motor não emite o fundo (`background: #eee`) de um `<img>`
cuja imagem não carregou. É um defeito de pintura do motor, o próximo alvo.

**2026-09-04, lote img-fundo:** `claude-object-fit` 1,95 % → 1,22 % (o fundo do
`<img>` pinta). O que fica é a IMAGEM: um `data:` de 1×1 que o loader do lado
TS não entrega ao `<img>` — não é do rasterizador nem do layout.

**2026-09-04, lote V-img:** o rasterizador pinta `DisplayItem::Pixels` (o
`<canvas>` e o `<img>` com pixels no documento) e MASCARA os `<img>` com `src`
e sem pixels — ele não tem a ponte, e é a ponte que descodifica o PNG de
`data:`. `claude-object-fit`: 0 % diferente com 1,95 % de área mascarada;
`claude-img-natural` 0,08 %. As imagens só se medem de verdade pela janela do
egui, que tem a ponte.

**2026-09-04, lote V-img-2:** `claude-img-ficheiro` fica em 1 % e é o
INSTRUMENTO: o rasterizador não lê ficheiros (não tem a ponte), o `#solto` de
6×4 sai 0×0 e a linha seguinte sobe 4px — 2×4×1280 pixels. A caixa mede certo
no corpus de layout (88/92); a pintura de imagens de ficheiro só se verifica
pela janela do egui.
