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

**Imagem** (`<img>`, `background-image` com URL), por agora. `DisplayItem::Image`
aponta para um handle do `HandleTable`, que o exemplo não tem (não instancia
`Engine`/`Registry`, só `Dom`+layout — trazê-los para um exemplo de
rasterização é o preço de pintar UMA fixture no corpus atual). Nenhuma
fixture de `tests/css/` depende disto para o que fixa; quando uma depender,
o custo de instanciar o handle table paga-se então.

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
