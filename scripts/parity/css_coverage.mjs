// COBERTURA DE PROPRIEDADES CSS — quantas das que as folhas reais escrevem o
import fs from 'fs';
// motor reconhece, contadas por PROPRIEDADE, por DECLARAÇÃO e por FOLHA.
//
//   node scripts/parity/css_coverage.mjs          # corpus alargado (13 folhas)
//   node scripts/parity/css_coverage.mjs --antigo # o corpus de 4, para comparar
//
// Existe para o número ser RE-MEDIDO em vez de citado. O que substituiu foi a
// linha "68 de 363 usadas" de `docs/ui/estado-motor-css.md`, que ninguém sabia
// reproduzir — um número sem instrumento é uma afirmação.
//
// ## Armadilhas de DENOMINADOR — e é por causa delas que isto é um ficheiro
//
// 1. `scripts/parity/pagina.combinada.html` JÁ EMBUTE `pagina.css`. Contar os
//    dois dá tudo a dobrar (dá para reconhecer: todos os totais saem PARES).
//    Aqui entra só a combinada.
// 2. O motor reconhece nomes por FORMA e não só por literal: `parse.rs` tem um
//    braço-guarda para as doze longhands `border-<lado>-<width|style|color>`, e
//    `style/radius.rs` e `style/logical.rs` reconhecem famílias inteiras. Uma
//    varredura só por literais declara-as em falta — e mandaria "implementar"
//    doze propriedades que já existem.
// 3. **Folhas repetidas inflacionam a cobertura sem alargar o corpus.** O
//    corpus antigo dizia "quatro folhas" e eram três: `wa.css` e `wa-app.css`
//    são BYTE-A-BYTE o mesmo ficheiro. Ao alargar apareceram mais três casos —
//    `wiki-fisica` e `wiki-rust-en` são MediaWiki outra vez (jaccard 0,95 e
//    0,88 sobre o conjunto de propriedades) e `bootstrap-cover.css` tem
//    exatamente o mesmo conjunto de propriedades que `bootstrap.css` (jaccard
//    1,00). Nenhuma das quatro está na lista abaixo: uma folha repetida vota
//    duas vezes na coluna "em N folhas", que é a coluna que existe para dizer
//    se uma propriedade é geral ou é o hábito de um autor.
//
// Os nomes reconhecidos são extraídos da FONTE (os braços do `match`), nunca de
// uma lista mantida aqui: uma segunda lista seria mais uma coisa a dessincronizar.
// As famílias reconhecidas por forma estão abaixo, explícitas, porque não há
// literal de onde as ler.
//
// ## Três colunas, e a diferença entre elas é o ponto
//
// - RECONHECIDA: parseada e guardada. DIVIDE-SE em duas, porque "reconhecida"
//   juntava `background-position-x`, que o render pinta, com `object-fit`, que
//   e parseada, respondida pelo computado e nao muda um pixel:
//     * EFETIVA  — lida por quem desenha (ver o bloco "TEM CONSUMIDOR?").
//     * GUARDADA — parseada e guardada, ninguem que desenha a le.
//   E uma terceira, pequena e visivel de proposito: INDETERMINADA, para o que e
//   reconhecido por um caminho sem campo na tabela. Sao tres hoje: `content`
//   (servida pelo caminho do pseudo-elemento, e DESENHADA — logo esta subcontada
//   e nao sobrecontada) e `animation-fill-mode`/`animation-play-state`, que sao
//   reconhecidas e nao escrevem nada por decisao. Preferi deixa-las a aparecer
//   do que arrumadas numa coluna que nao lhes serve.
// - RECUSADA (`style/inert.rs`): reconhecida e deliberadamente não modelada.
// - DESCONHECIDA: a lista do que falta fazer. É esta que tem de ser lida.
//
// Sem a coluna do meio, `will-change` (que nunca vai ter efeito) somava com
// `object-fit` (que é trabalho a fazer), e o total não media nada.
//
// ## Duas ordenações, porque uma sozinha engana
//
// A lista final sai ordenada por FOLHAS primeiro e por declarações a seguir.
// Uma folha de utilitários (Tailwind) escreve a mesma propriedade dez mil
// vezes: por declarações, `--tw-*`-adjacentes e `grid-column` empurram tudo o
// resto para baixo por causa de UM autor. O número de folhas distintas é que
// diz se a web escreve aquilo.
const D = 'E:/rts/crates/rts-dom/src/style/';
const known = new Set();

// ── EXTRAÇÃO DOS NOMES RECONHECIDOS ─────────────────────────────────────────
//
// ## Porque isto não pode olhar para a INDENTAÇÃO
//
// A versão anterior lia os braços do `match` filtrando por um número fixo de
// espaços à esquerda (`['parse.rs', 12]`). Isso mediu bem até ao dia em que o
// ficheiro foi reformatado e a função extraída: os braços passaram de 12 para 8
// espaços e a sonda respondeu **103/364 (28,3%) em vez de 211/364, e 12,9% de
// declarações em vez de 96,8%** — sem uma linha do motor ter mudado. Um
// instrumento que responde ao ESTILO do ficheiro não mede o motor.
//
// O critério passou a ser a PROFUNDIDADE DE CHAVETAS, que é a estrutura do
// código e não a sua apresentação: `rustfmt` pode mover o texto à vontade, mas
// não pode mudar quantas chavetas estão abertas num ponto.
//
// ## Porque não basta apanhar todo o `"x" =>` do ficheiro
//
// `=>` em Rust só aparece em braços de match e em macros, o que faria dele um
// bom sinal — se não houvesse matches ANINHADOS. `vocab.rs` tem
// `"box-orient" => { … match val { "vertical" => … } }`, e `painting.rs` tem os
// `kw!(BlendMode { "multiply" => … })`. Apanhar tudo daria `vertical`,
// `multiply` e `solid` como propriedades CSS reconhecidas — a sonda a inventar
// cobertura.
//
// A regra que resolve os dois: dentro da função de despacho, os braços que
// interessam são os do match MAIS EXTERIOR, e esses são todos os que estão na
// profundidade MÍNIMA. A mínima é calculada a partir do próprio ficheiro, não
// escrita aqui — se a função ganhar um nível de aninhamento amanhã, a sonda
// acompanha sozinha.

/// O corpo de `fn <nome>` — do `{` de abertura até à chaveta que o fecha.
function corpoDaFuncao(src, nome) {
  // Procura literal em vez de expressão regular: o padrão viveria num template
  // literal, onde `\b` e `\s` são escapes da STRING antes de chegarem à regex —
  // e uma regex que se transforma em `fns+nome` silenciosamente não encontra
  // nada e faz a sonda responder zero com ar de resposta.
  const at = src.indexOf(`fn ${nome}`);
  if (at < 0) {
    throw new Error(
      `css_coverage: fn ${nome} não encontrada — a sonda está a medir uma fonte que mudou de forma`
    );
  }
  let i = src.indexOf('{', at);
  let d = 0;
  for (let j = i; j < src.length; j++) {
    if (src[j] === '{') d++;
    else if (src[j] === '}') { d--; if (d === 0) return src.slice(i + 1, j); }
  }
  throw new Error(`css_coverage: fn ${nome} sem fecho`);
}

function bracosComCorpo(corpo) {
  const limpo = corpo.replace(/\/\/[^\n]*/g, '');
  const achados = [];
  let d = 0;
  const re = /(?:"[a-zA-Z-]+"\s*\|\s*)*"[a-zA-Z-]+"\s*=>/g;
  let pos = 0;
  while (pos < limpo.length) {
    const c = limpo[pos];
    if (c === '{') { d++; pos++; continue; }
    if (c === '}') { d--; pos++; continue; }
    if (c === '"') {
      re.lastIndex = pos;
      const m = re.exec(limpo);
      if (m && m.index === pos) { achados.push([d, pos, re.lastIndex, m[0]]); pos = re.lastIndex; continue; }
      const fim = limpo.indexOf('"', pos + 1);
      pos = fim < 0 ? limpo.length : fim + 1;
      continue;
    }
    pos++;
  }
  if (!achados.length) return [];
  const min = Math.min(...achados.map(([d]) => d));
  const doTopo = achados.filter(([d]) => d === min);
  return doTopo.map(([, ini, fim, txt], k) => ({
    nomes: txt.match(/"[^"]+"/g).map(q => q.slice(1, -1)),
    corpo: limpo.slice(fim, k + 1 < doTopo.length ? doTopo[k + 1][1] : limpo.length),
  }));
}

/// Só os NOMES dos braços do match exterior. Uma vista de `bracosComCorpo` e não
/// uma segunda varredura: as duas perguntas ("que nomes" e "que campos") têm de
/// concordar sobre o que é um braço, senão a cobertura e a classificação por
/// consumidor discordam sobre a mesma propriedade.
function bracosDoMatchExterior(corpo) {
  return bracosComCorpo(corpo).flatMap(b => b.nomes);
}
// A função de despacho de cada módulo, e se ele TIRA o prefixo de fornecedor
// antes de casar. O nome da função é estrutura; a indentação não era.
const FONTES = [
  ['parse.rs', 'aplica_declaracao'],
  ['timing.rs', 'try_apply'],
  ['vocab.rs', 'try_apply'],
  ['grid_lines.rs', 'try_apply'],
  ['painting.rs', 'try_apply'],
  // `logical.rs` tem DUAS tabelas de nome→nome em braços de match — as dimensões
  // lógicas (`inline-size` → `width`) e os nomes antigos do WebKit
  // (`margin-end` → `margin-inline-end`). São reconhecimento como qualquer
  // outro; ficarem de fora punha `inline-size`, `block-size`, `min-inline-size`
  // e `-webkit-margin-end` na lista do que falta fazer, já implementadas.
  // As famílias que ele serve por FORMA (`border-inline-start-color`) não têm
  // literal e continuam na tabela de formas mais abaixo.
  ['logical.rs', 'try_apply'],
];
for (const [f, fn] of FONTES) {
  const nomes = bracosDoMatchExterior(corpoDaFuncao(fs.readFileSync(D + f, 'utf8'), fn));
  if (!nomes.length) throw new Error(`css_coverage: ${f}::${fn} não deu braço nenhum`);
  for (const n of nomes) known.add(n);
}

// ── PREFIXOS DE FORNECEDOR ──────────────────────────────────────────────────
//
// Já não há uma lista de quais módulos "tiram o prefixo": `parse.rs` faz uma
// ÚLTIMA tentativa com o nome sem prefixo, e essa tentativa reentra em toda a
// cadeia. A regra do motor passou a ser uma só — **se o nome nu é reconhecido,
// o prefixado também é** — e é essa que a sonda lê.
//
// Com uma exceção, que o motor tem explícita e a sonda tem de ter igual: as
// duas sintaxes ANTIGAS de flexbox não são aliases (`-ms-flex-pack: justify` é
// `justify-content: space-between`), e `inert::flexbox_de_2009` recusa-as antes
// do corte do prefixo. Dá-las por reconhecidas seria a sonda a contar como feito
// aquilo que o motor recusa de propósito.
const PREFIXOS = ['-webkit-', '-moz-', '-ms-', '-o-'];
/// A exceção é SÓ a família `-ms-flex*`, e a razão é a ORDEM da cadeia do motor.
///
/// `inert::flexbox_de_2009` também nomeia `-webkit-box-*`, mas essas não
/// precisam de exceção aqui e pô-las seria um falso negativo: metade delas
/// (`box-orient`, `box-pack`, `box-align`) é traduzida pelo `style::vocab`, que
/// corre ANTES do `inert` — o motor reconhece-as. A outra metade (`box-flex`,
/// `box-direction`, `box-ordinal-group`) não tem nome nu reconhecido, portanto
/// já não entra por esta porta.
///
/// A `-ms-flex*` é diferente porque o nome nu É reconhecido: `flex`,
/// `flex-direction` e `flex-wrap` estão todos no `parse`. Sem esta exceção a
/// sonda daria `-ms-flex-pack` por coberto — e o motor recusa-o de propósito,
/// porque `-ms-flex-pack: justify` é `justify-content: space-between` e traduzir
/// por prefixo daria o valor errado em silêncio.
function ehFlexboxDe2009(prop) {
  return prop === '-ms-flex' || prop.startsWith('-ms-flex-');
}
for (const p of [...known]) {
  for (const pre of PREFIXOS) {
    const c = pre + p;
    if (!ehFlexboxDe2009(c)) known.add(c);
  }
}
// borders::is_longhand (guarda por FORMA) e logical (tradução de eixo).
for (const s of ['top', 'right', 'bottom', 'left']) {
  known.add('border-' + s);
  for (const w of ['width', 'style', 'color']) known.add(`border-${s}-${w}`);
}
for (const l of ['inline-start', 'inline-end', 'block-start', 'block-end']) {
  known.add('inset-' + l); known.add('border-' + l);
  for (const w of ['width', 'style', 'color']) known.add(`border-${l}-${w}`);
}
for (const x of ['inset', 'inset-inline', 'inset-block']) known.add(x);
// A caixa lógica por FORMA: `logical::try_apply` traduz o eixo e reentrega ao
// `parse` com o nome físico, portanto não há braço literal destes nomes.
for (const l of ['inline-start', 'inline-end', 'block-start', 'block-end']) {
  known.add('padding-' + l);
  known.add('margin-' + l);
}
// style/radius.rs: reconhece pela FORMA border-<canto>-radius.
for (const c of ['top-left', 'top-right', 'bottom-right', 'bottom-left', 'start-start', 'start-end', 'end-end', 'end-start']) known.add('border-' + c + '-radius');
// PROPRIEDADES SERVIDAS POR CAMINHO PRÓPRIO, fora do `match` de `parse.rs`.
//
// `content` não é uma declaração como as outras: só tem sentido num `::before`/
// `::after`, por isso é lida do corpo da regra em `stylesheet.rs`
// (`content_do_corpo` → `pseudo::parse_content`) e nunca chega ao `match`. Uma
// varredura só dos braços do `match` declarava-a EM FALTA — e era a maior
// entrada da lista, 237 declarações em 10 das 13 folhas. Um falso negativo do
// tamanho da primeira linha da lista de trabalho.
//
// A leitura é da FONTE pela mesma razão que a dos braços do `match`: escrever
// `known.add('content')` aqui resolvia hoje e ficava a mentir no dia em que o
// caminho próprio mudasse de nome. O padrão procurado é a comparação do NOME da
// declaração (`nome`/`prop`), que é o que distingue `eq_ignore_ascii_case` sobre
// uma propriedade de `eq_ignore_ascii_case` sobre um valor (`"from"`/`"to"` de
// um `@keyframes`, na mesma folha).
for (const line of fs.readFileSync(D + 'stylesheet.rs', 'utf8').split('\n')) {
  if (!/\b(nome|prop)\b[^=]*\.eq_ignore_ascii_case\(/.test(line)) continue;
  for (const m of line.matchAll(/eq_ignore_ascii_case\("([a-z-]+)"\)/g)) known.add(m[1]);
}

// ════════════════════════════════════════════════════════════════════════════
// TEM CONSUMIDOR? — a divisão da coluna "reconhecidas" em duas.
//
// "Reconhecida" juntava duas coisas muito diferentes: `background-position-x`,
// que o render já pinta, e `object-fit`, que é parseada, guardada, respondida
// pelo computado e não muda um pixel. É a mesma doença que o `inert.rs` veio
// curar, um degrau acima — e não se via porque as duas cabiam na mesma palavra.
//
// ## O critério, e as duas armadilhas que ele tem de evitar
//
// **Não é "lido fora de `style/`".** `border_top_style` decide a largura USADA
// de um lado (um lado sem estilo ocupa zero), e quem o lê é
// `borders::resolved_sides`, DENTRO de `style/`. Por esse critério a borda por
// lado saía como "sem consumidor" tendo o layout inteiro dependente dela.
//
// **Nem "lido em qualquer lado".** `fmt.rs` lê TODOS os campos para responder o
// `getComputedStyle`, e `parse.rs` escreve-os todos. Deixar a superfície entrar
// marca tudo como efetivo: a primeira corrida desta análise deu 111 de 117, com
// `text-overflow` do lado errado.
//
// O critério é: **um campo é EFETIVO se for lido por quem DESENHA** — os
// ficheiros de geometria e de pintura — **ou por uma função de `style/` que esse
// código chama** (fecho transitivo). Quem desenha é uma lista de FICHEIROS,
// curta e estável, e não uma lista por propriedade: é um facto do projeto.
// A exclusão da superfície é por VERBO (`parse_*`, `apply_*`, `get_property`,
// `computed_value`, `initial`, `fmt_*`) e é a definição da pergunta, não uma
// lista a manter.
//
// ## Três defeitos que esta análise teve antes de dar um número
//
// 1. `dom.rs` não estava na lista de quem desenha, e é ele que corre o laço de
//    animação — `transition` e `animation` saíam como "sem consumidor" tendo o
//    consumidor mais visível do motor.
// 2. Uma cadeia de métodos parte a linha e deixa o campo sozinho (`layout.rs`
//    tem `.text_indent` com o recetor na linha anterior). A primeira passagem,
//    que exige `recetor.campo`, perdia-o. A segunda passagem apanha o campo sem
//    recetor, mas SÓ para nomes não ambíguos.
// 3. `size.width` e `run.color` não provam nada: os nomes ambíguos existem em
//    structs de geometria. Um recetor só conta como estilo se aparecer ao menos
//    uma vez com um campo que só pode ser de estilo.
const CRATES = 'E:/rts/crates';
const AMBIGUOS = new Set(['width', 'height', 'color', 'position', 'opacity', 'transform',
  'filter', 'order', 'display', 'gap', 'visibility', 'clear', 'bold', 'italic', 'font_size']);

function ficheirosRs(dir) {
  const out = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = dir + '/' + e.name;
    if (e.isDirectory()) { if (e.name !== 'target') out.push(...ficheirosRs(p)); }
    else if (e.name.endsWith('.rs')) out.push(p);
  }
  return out;
}
const TODOS = ficheirosRs(CRATES);

// Os campos da tabela `css_props!` — a fonte única do que é uma propriedade.
const CAMPOS = new Set();
{
  const t = fs.readFileSync(D + 'props.rs', 'utf8');
  for (const m of t.matchAll(/^\s*\[[a-z ]*\]\s*([a-z_0-9]+)\s*[:;]/gm)) CAMPOS.add(m[1]);
  // Os campos BUILT-IN da macro (`pub grid_template_rows: ...`) nao sao linhas da
  // tabela `[..] nome: tipo;` e escapavam a esta leitura — `grid-template-rows` e
  // `justify-items` saiam como "sem campo conhecido" tendo campo e consumidor.
  for (const m of t.matchAll(/^\s*pub\s+([a-z_0-9]+)\s*:/gm)) CAMPOS.add(m[1]);
}

/** Quem DESENHA: geometria e pintura. O resto é superfície. */
function desenha(p) {
  if (p.includes('/style/')) return false;
  if (/\/rts-(egui|render|ui)\//.test(p)) return true;
  if (p.includes('/table/')) return true;
  return ['layout.rs', 'inline_box.rs', 'block.rs', 'listitem.rs', 'scrollbar.rs',
    'anim.rs', 'pseudo.rs', 'flowtests.rs', 'dom.rs'].some(n => p.endsWith('/' + n));
}

/** Campos LIDOS (não escritos) num texto. Ver as três armadilhas acima. */
function leituras(texto) {
  const porRecv = new Map(), brutos = [];
  for (const m of texto.matchAll(/([A-Za-z_][A-Za-z_0-9]*)\.([a-z_0-9]+)\b(?!\s*\()/g)) {
    if (!CAMPOS.has(m[2])) continue;
    if (/^\s*=[^=]/.test(texto.slice(m.index + m[0].length, m.index + m[0].length + 3))) continue;
    if (!porRecv.has(m[1])) porRecv.set(m[1], new Set());
    porRecv.get(m[1]).add(m[2]);
    brutos.push([m[1], m[2]]);
  }
  const ok = new Set([...porRecv].filter(([, cs]) => [...cs].some(c => !AMBIGUOS.has(c))).map(([r]) => r));
  const achados = new Set(brutos.filter(([r]) => ok.has(r)).map(([, c]) => c));
  for (const m of texto.matchAll(/\.([a-z_0-9]+)\b(?!\s*\()/g)) {
    if (!CAMPOS.has(m[1]) || AMBIGUOS.has(m[1])) continue;
    if (/^\s*=[^=]/.test(texto.slice(m.index + m[0].length, m.index + m[0].length + 3))) continue;
    achados.add(m[1]);
  }
  return achados;
}

const efetivos = new Set();
const chamadasDeQuemDesenha = new Set();
for (const p of TODOS.filter(desenha)) {
  const t = fs.readFileSync(p, 'utf8');
  for (const c of leituras(t)) efetivos.add(c);
  for (const m of t.matchAll(/\b([a-z_][a-z_0-9]*)\s*\(/g)) chamadasDeQuemDesenha.add(m[1]);
}

// As funções de `style/`: o que cada uma lê e o que chama.
const SUPERFICIE = /^(parse|apply|fmt|set_side|split|is_|to_)|^(get_property|computed_value|initial|try_apply|css)$/;
// Quantas VEZES cada nome de funcao e definido em style/. `parse` esta definida
// dezenas de vezes (uma por tipo de valor), e seguir uma chamada por NOME juntava
// o que todas elas escrevem — `align-content` virava "efetiva" por causa do
// `BorderStyle::parse` de outro ficheiro. So se segue nome com UMA definicao.
const defsPorNome = new Map();
const leDaFn = new Map(), chamaFn = new Map(), escreveDaFn = new Map(), fnsStyle = new Set();
for (const p of TODOS.filter(p => p.includes('/style/'))) {
  const t = fs.readFileSync(p, 'utf8');
  for (const m of t.matchAll(/\bfn\s+([a-z_][a-z_0-9]*)\s*[(<]/g)) {
    const i = t.indexOf('{', m.index + m[0].length);
    if (i < 0) continue;
    let d = 0, fim = -1;
    for (let j = i; j < t.length; j++) {
      if (t[j] === '{') d++;
      else if (t[j] === '}') { d--; if (d === 0) { fim = j; break; } }
    }
    if (fim < 0) continue;
    const corpo = t.slice(i, fim);
    fnsStyle.add(m[1]);
    defsPorNome.set(m[1], (defsPorNome.get(m[1]) || 0) + 1);
    if (!leDaFn.has(m[1])) leDaFn.set(m[1], new Set());
    for (const c of leituras(corpo)) leDaFn.get(m[1]).add(c);
    if (!escreveDaFn.has(m[1])) escreveDaFn.set(m[1], new Set());
    for (const w of corpo.matchAll(/css\.([a-z_0-9]+)/g)) if (CAMPOS.has(w[1])) escreveDaFn.get(m[1]).add(w[1]);
    if (!chamaFn.has(m[1])) chamaFn.set(m[1], new Set());
    for (const g of corpo.matchAll(/\b([a-z_][a-z_0-9]*)\s*\(/g)) chamaFn.get(m[1]).add(g[1]);
  }
}
{
  const fila = [...fnsStyle].filter(f => chamadasDeQuemDesenha.has(f) && !SUPERFICIE.test(f));
  const vistas = new Set();
  while (fila.length) {
    const f = fila.pop();
    if (vistas.has(f)) continue;
    vistas.add(f);
    for (const c of leDaFn.get(f) || []) efetivos.add(c);
    for (const g of chamaFn.get(f) || []) if (fnsStyle.has(g) && !SUPERFICIE.test(g) && !vistas.has(g)) fila.push(g);
  }
}

// NOME CSS → campos que ele escreve, lido dos corpos dos braços do `match`.
const camposDoNome = new Map();
// A MESMA extração dos nomes (profundidade de chavetas), agora com o corpo de
// cada braço. Isto era por indentação fixa e partiu-se com a mesma reformatação
// que partiu a outra leitura — duas regras diferentes para ler a mesma coisa é
// o defeito que este ficheiro passou o dia a corrigir noutros sítios.
for (const [f, fn] of FONTES) {
  for (const braco of bracosComCorpo(corpoDaFuncao(fs.readFileSync(D + f, 'utf8'), fn))) {
    const corpo = braco.corpo;
    const escreve = new Set([...corpo.matchAll(/css\.([a-z_0-9]+)/g)].map(x => x[1]).filter(c => CAMPOS.has(c)));
    // DELEGACAO: um braco pode nao escrever campo nenhum e chamar quem escreve
    // (`apply_border_shorthand(css, val)`). Segue-se a chamada, com fecho dentro
    // de style/ — `border-color` delega em `apply_color_shorthand`, que delega
    // em `set_side_color`, e so o segundo salto chega ao campo.
    {
      const unica = f => fnsStyle.has(f) && defsPorNome.get(f) === 1;
      const fila = [...corpo.matchAll(/\b([a-z_][a-z_0-9]*)\s*\(/g)].map(x => x[1]).filter(unica);
      const vistas = new Set();
      while (fila.length) {
        const f = fila.pop();
        if (vistas.has(f)) continue;
        vistas.add(f);
        for (const c of escreveDaFn.get(f) || []) escreve.add(c);
        for (const g of chamaFn.get(f) || []) if (unica(g) && !vistas.has(g)) fila.push(g);
      }
    }
    for (const nm of braco.nomes) {
      if (!camposDoNome.has(nm)) camposDoNome.set(nm, new Set());
      for (const c of escreve) camposDoNome.get(nm).add(c);
    }
  }
}
// As famílias reconhecidas por FORMA não têm braço literal de onde ler o campo.
// Os campos vêm do MÓDULO que as serve — a união do que ele escreve —, que é a
// mesma leitura da fonte, com a granularidade que a forma permite.
for (const [mod, nomes] of [
  ['borders.rs', [...['top', 'right', 'bottom', 'left'], ...['inline-start', 'inline-end', 'block-start', 'block-end']]
    .flatMap(s => ['border-' + s, ...['width', 'style', 'color'].map(w => `border-${s}-${w}`)])],
  ['radius.rs', ['top-left', 'top-right', 'bottom-right', 'bottom-left', 'start-start', 'start-end', 'end-end', 'end-start'].map(c => `border-${c}-radius`)],
  // `logical.rs` serve estas por FORMA (traduz o eixo e reentrega), sem literal
  // de onde as ler. A caixa lógica está aqui inteira de propósito: era
  // assimétrica no motor — `margin-block-end` funcionava e `padding-block-end`
  // não — e listar só metade aqui reproduzia a assimetria na medição.
  ['logical.rs', [
    'inset', 'inset-inline', 'inset-block',
    ...['inline-start', 'inline-end', 'block-start', 'block-end'].flatMap(l =>
      ['inset-' + l, 'padding-' + l, 'margin-' + l]),
  ]],
]) {
  const t = fs.readFileSync(D + mod, 'utf8');
  const escreve = new Set([...t.matchAll(/css\.([a-z_0-9]+)/g)].map(x => x[1]).filter(c => CAMPOS.has(c)));
  for (const nm of nomes) {
    if (!camposDoNome.has(nm)) camposDoNome.set(nm, new Set());
    for (const c of escreve) camposDoNome.get(nm).add(c);
  }
}
// Prefixos de fornecedor: escrevem o MESMO campo do nome nu, porque é
// literalmente a mesma função que os aplica (ver `PREFIXOS` acima).
for (const p of [...camposDoNome.keys()])
  for (const pre of PREFIXOS)
    if (!ehFlexboxDe2009(pre + p)) camposDoNome.set(pre + p, camposDoNome.get(p));

/**
 * 'efetiva' | 'parcial' | 'guardada' | 'indeterminada' (sem campo conhecido).
 *
 * A classe PARCIAL existe porque a resposta binária escondia metade de uma
 * propriedade. Isto respondia por OR — bastava UM campo lido para a propriedade
 * inteira contar como efetiva — e a nossa tabela é 1 propriedade → N campos, ao
 * contrário da do Blink. `background-image` escreve `bg_image` E `gradient`: o
 * gradiente é pintado, o `url()` não tem leitor nenhum, e a propriedade era
 * dada como efetiva. Com ela ia escondida a família que depende do `url()` —
 * `background-position`, `-size`, `-repeat`, `-clip`, `-origin`, `-attachment`,
 * 351 declarações no corpus, todas atrás de um campo que ninguém lê.
 *
 * Inverter para AND seria trocar um erro pelo simétrico: `background-image`
 * passaria a "guardada" e o gradiente que É pintado desaparecia da conta. Uma
 * propriedade meio implementada não é nenhuma das duas coisas, e a régua tem de
 * o dizer em vez de escolher o lado que soa melhor.
 */
function consumo(prop) {
  const cs = camposDoNome.get(prop);
  if (!cs || cs.size === 0) return 'indeterminada';
  const lidos = [...cs].filter(c => efetivos.has(c)).length;
  if (lidos === 0) return 'guardada';
  return lidos === cs.size ? 'efetiva' : 'parcial';
}

/** Os campos de uma propriedade PARCIAL que ninguém lê — o que ela esconde. */
function camposOrfaos(prop) {
  return [...(camposDoNome.get(prop) || [])].filter(c => !efetivos.has(c));
}

// ── VERIFICAÇÃO DA PRÓPRIA SONDA ────────────────────────────────────────────
//
// Casos conhecidos, afirmados antes de a sonda medir seja o que for. Existem
// porque a extração já falhou de TRÊS maneiras diferentes num só dia — a
// indentação mudou, um módulo novo não estava na lista, um `-ms-` faltava no
// corte do prefixo — e das três vezes o sintoma foi o mesmo: **um número mais
// baixo, com ar de resposta.** Uma sonda que responde 28,3% em vez de 66,8% por
// se ter partido é pior que uma sonda que não corre, porque a primeira é
// citável.
//
// Cada linha é uma FORMA de reconhecimento diferente. Se uma delas cair, o
// defeito está aqui e não no motor, e é isso que a mensagem tem de dizer — um
// `assert` mudo mandaria procurar no sítio errado.
for (const [prop, porque] of [
  ['display', 'braço literal do match principal (parse.rs)'],
  ['border-bottom-color', 'família por FORMA (borders::is_longhand)'],
  ['content', 'caminho próprio, fora do match (stylesheet.rs)'],
  ['border-top-left-radius', 'família por FORMA (style/radius.rs)'],
  ['inset-inline-start', 'tradução de eixo lógico (style/logical.rs)'],
  ['object-fit', 'módulo ligado por `_ if` (style/vocab.rs)'],
  ['grid-column-start', 'módulo ligado por `_ if` (style/grid_lines.rs)'],
  ['background-clip', 'módulo ligado por `_ if` (style/painting.rs)'],
  ['transition-duration', 'módulo ligado por `_ if` (style/timing.rs)'],
  ['-webkit-box-shadow', 'prefixo de fornecedor sobre um nome nu conhecido'],
]) {
  if (!known.has(prop)) {
    throw new Error(
      `css_coverage: \`${prop}\` devia ser reconhecida (${porque}) e não está.
` +
      `  A SONDA está partida, não o motor — a extração deixou de ver esta forma.`
    );
  }
}
// E o reverso, que é onde uma extração larga demais se esconde: um valor de
// keyword não é uma propriedade. Se `vertical` aparecer aqui, a leitura passou
// a apanhar braços de matches ANINHADOS e a cobertura está inflacionada.
for (const naoDeviaSer of ['vertical', 'multiply', 'ellipsis', 'border-box', 'wavy']) {
  if (known.has(naoDeviaSer)) {
    throw new Error(
      `css_coverage: \`${naoDeviaSer}\` é um VALOR e está a contar como propriedade.
` +
      `  A extração apanhou um match aninhado — a cobertura sai inflacionada.`
    );
  }
}
// E a recusa deliberada continua a ser recusa, não cobertura.
for (const recusada of ['-ms-flex', '-ms-flex-pack']) {
  if (known.has(recusada)) {
    throw new Error(`css_coverage: \`${recusada}\` é recusada pelo motor e a sonda dá-a por coberta.`);
  }
}

// as recusadas explicitamente (style/inert.rs), contadas a PARTE.
const inert = new Set();
// `is_inert` responde `true` por DOIS caminhos, e a sonda só lia um. O outro é
// `flexbox_de_2009`, uma função à parte que corre ANTES do `matches!` — são 15
// nomes e 268 declarações (`-ms-flex-pack`, `-webkit-box-flex`…) que o motor
// recusa de propósito e que apareciam como trabalho por fazer. Ler só metade da
// função fazia a lista do que falta ser 20 quando é 1.
for (const p of ['-ms-flex', '-ms-flex-align', '-ms-flex-direction', '-ms-flex-flow',
  '-ms-flex-item-align', '-ms-flex-line-pack', '-ms-flex-negative', '-ms-flex-order',
  '-ms-flex-pack', '-ms-flex-positive', '-ms-flex-preferred-size', '-ms-flex-wrap']) inert.add(p);
for (const p of ['flex', 'orient', 'direction', 'align', 'pack', 'ordinal-group', 'lines'])
  inert.add('-webkit-box-' + p);
{
  const t = fs.readFileSync(D + 'inert.rs', 'utf8');
  const body = t.slice(t.indexOf('matches!('));
  // `is_inert` tira TRÊS prefixos, não dois: `-webkit-`, `-moz-` e `-ms-`. A
  // sonda gerava só os dois primeiros e punha `-ms-user-select` (3 folhas) e
  // `-ms-touch-action` na lista do que falta fazer — nomes que o motor já
  // recusa com motivo. `-o-` não está aqui porque o código não o tira: gerá-lo
  // seria a sonda a afirmar uma capacidade que ninguém escreveu.
  for (const m of body.matchAll(/"([a-zA-Z-]+)"/g))
    for (const pre of ['', '-webkit-', '-moz-', '-ms-']) inert.add(pre + m[1]);
}

function cssOf(paths) {
  let out = '';
  for (const p of [].concat(paths)) {
    let t = fs.readFileSync(p, 'utf8');
    if (p.endsWith('.html')) { let c = ''; for (const m of t.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/gi)) c += m[1] + '\n'; t = c; }
    out += t.replace(/\/\*[\s\S]*?\*\//g, '') + '\n';
  }
  return out;
}
// Recolhe as declarações do texto que está DENTRO de chavetas mas fora de
// qualquer chaveta mais funda. A versão anterior guardava o bloco de nível 1
// inteiro e partia-o por `;`, o que num `@media` metia os SELETORES do interior
// no meio das declarações: `body:not(.intent-mouse) .x:focus,…{color:` passava
// no teste de nome e era contada como uma propriedade chamada `body`. Uma folha
// minificada com muitos `@media` — Primer, Bootstrap, Tailwind — enche assim o
// denominador com nomes que ninguém escreveu.
function scan(css) {
  const d = new Map();
  let depth = 0, buf = '';
  const flush = () => {
    for (const s of buf.split(';')) {
      const i = s.indexOf(':'); if (i < 0) continue;
      const p = s.slice(0, i).trim().toLowerCase();
      if (!/^-?[a-z][a-z0-9-]*$/.test(p) || p.startsWith('--')) continue;
      d.set(p, (d.get(p) || 0) + 1);
    }
    buf = '';
  };
  for (let i = 0; i < css.length; i++) {
    const c = css[i];
    if (c === '{') {
      // O que estava a ser acumulado era o SELETOR do bloco que agora abre, e
      // não uma declaração — descarta-se em vez de se contar.
      buf = ''; depth++; continue;
    }
    if (c === '}') { if (depth >= 1) flush(); depth--; if (depth < 0) depth = 0; continue; }
    if (depth >= 1) buf += c;
  }
  return d;
}

// O corpus antigo, guardado para a comparação e não para ser usado: mede o
// WhatsApp duas vezes (`wa-app.css` é o mesmo ficheiro que `wa.css`).
const ANTIGO = {
  mediawiki: 'E:/rts/scripts/parity/pagina.combinada.html',
  google: 'E:/rts/google.css',
  wa: 'E:/rts/wa.css',
  waapp: 'E:/rts/wa-app.css',
};
// O corpus alargado: um sítio por autor distinto. Os frameworks estão em
// `paridade/frameworks/` e foram descarregados do jsDelivr — a versão faz
// parte do nome do pedido, não do ficheiro, por isso está aqui.
const CORPUS = {
  mediawiki: 'E:/rts/scripts/parity/pagina.combinada.html',
  google: 'E:/rts/google.css',
  whatsapp: 'E:/rts/wa.css',
  hn: 'E:/rts/paridade/hn.css',
  pythondocs: 'E:/rts/paridade/python-docs.css',
  bootstrap5: 'E:/rts/paridade/frameworks/bootstrap5.css',      // bootstrap@5.3.3
  tailwind2: 'E:/rts/paridade/frameworks/tw2.css',              // tailwindcss@2.2.19 dist
  // tailwind@4 não publica utilities compiladas (são geradas); o que existe no
  // pacote é theme+preflight+index, e é isso que entra — dizer "Tailwind 4" a
  // partir de utilitários inventados seria medir contra um corpus imaginário.
  tailwind4: ['E:/rts/paridade/frameworks/tailwind4.css', 'E:/rts/paridade/frameworks/tw4-theme.css', 'E:/rts/paridade/frameworks/tw4-preflight.css'],
  bulma: 'E:/rts/paridade/frameworks/bulma.css',                // bulma@1.0.2
  foundation: 'E:/rts/paridade/frameworks/foundation.css',      // foundation-sites@6.9.0
  materialize: 'E:/rts/paridade/frameworks/materialize.css',    // @materializecss/materialize@2.2.2
  primer: 'E:/rts/paridade/frameworks/primer.css',              // @primer/css@21.5.1 (GitHub)
  fontawesome: 'E:/rts/paridade/frameworks/fontawesome.css',    // @fortawesome/fontawesome-free@6.7.2
};

// Descritores de at-rule: aparecem depois de `:` dentro de chavetas mas não são
// propriedades de nenhum elemento, por isso não são cobertura nem falta.
const desc = new Set(['syntax', 'inherits', 'initial-value', 'src', 'unicode-range', 'symbols',
  'suffix', 'speak', 'system', 'additive-symbols', 'negative', 'pad', 'range', 'fallback',
  'prefix', 'font-display', 'ascent-override', 'descent-override', 'line-gap-override', 'size-adjust']);

const srcs = process.argv.includes('--antigo') ? ANTIGO : CORPUS;
const uni = new Map();   // prop -> declarações totais
const folhas = new Map(); // prop -> nº de folhas distintas em que aparece
for (const [k, p] of Object.entries(srcs)) {
  const d = scan(cssOf(p));
  let dn = 0, dk = 0;
  for (const [prop, n] of d) {
    dn += n; if (known.has(prop)) dk += n;
    if (desc.has(prop)) continue;
    uni.set(prop, (uni.get(prop) || 0) + n);
    folhas.set(prop, (folhas.get(prop) || 0) + 1);
  }
  console.log(k.padEnd(12), 'props', String(d.size).padStart(4),
    'reconhecidas', String([...d.keys()].filter(x => known.has(x)).length).padStart(4),
    ' declaracoes', String(dn).padStart(7), 'cobertas', String(dk).padStart(7),
    (100 * dk / dn).toFixed(1) + '%');
}
const rows = [...uni];
const tot = rows.length, rec = rows.filter(([p]) => known.has(p)).length;
const dn = rows.reduce((a, [, n]) => a + n, 0), dk = rows.filter(([p]) => known.has(p)).reduce((a, [, n]) => a + n, 0);
console.log(`\nUNIAO (${Object.keys(srcs).length} folhas, sem descritores de at-rule): ${rec}/${tot} propriedades (${(100 * rec / tot).toFixed(1)}%), ${dk}/${dn} declaracoes (${(100 * dk / dn).toFixed(1)}%)`);
// A coluna "reconhecidas" DIVIDIDA: ver o bloco "TEM CONSUMIDOR?" acima.
const conhecidas = rows.filter(([p]) => known.has(p));
const porConsumo = { efetiva: [], parcial: [], guardada: [], indeterminada: [] };
for (const r of conhecidas) porConsumo[consumo(r[0])].push(r);
const soma = a => a.reduce((x, [, n]) => x + n, 0);
console.log(`  EFETIVAS  (todos os campos lidos):    ${String(porConsumo.efetiva.length).padStart(3)} propriedades, ${soma(porConsumo.efetiva)} declaracoes`);
console.log(`  PARCIAIS  (uns lidos, outros nao):    ${String(porConsumo.parcial.length).padStart(3)} propriedades, ${soma(porConsumo.parcial)} declaracoes`);
console.log(`  GUARDADAS (parseadas, ninguem as le): ${String(porConsumo.guardada.length).padStart(3)} propriedades, ${soma(porConsumo.guardada)} declaracoes`);
// As parciais listam-se por inteiro, com o campo orfao ao lado: sao a classe
// que a resposta binaria escondia, e um numero agregado voltaria a escondê-las.
// Uma parcial cujos campos órfãos não partilham UM token com o nome da
// propriedade é quase de certeza sobre-recolha do mapa nome→campos, e não uma
// falha do motor: o mapa segue as funções chamadas por cada braço, e um braço
// que chame um ajudante partilhado herda o que esse ajudante escreve. `grid-gap`
// a aparecer sem leitor de `bg_image` é isso, não um defeito de grid.
//
// A linha imprime-se na mesma, marcada. Silenciá-la esconderia o defeito do
// instrumento, que é o que a classe PARCIAL acabou de servir para revelar — a
// resposta binária escondia-o por completo, dos dois lados.
// Os campos abreviam onde o nome CSS não abrevia (`bg_image` para
// `background-image`), portanto a comparação de tokens precisa dos sinónimos —
// sem eles, `background` era marcado como sobre-recolha por ter campos `bg_*`,
// que é a única família onde a marca estaria errada.
const SINONIMOS = { bg: 'background', valign: 'vertical', decoration: 'decoration' };
const tokens = s => new Set(s.replace(/^-(webkit|moz|ms|o)-/, '').split(/[-_]/)
  .map(t => SINONIMOS[t] || t));
const suspeita = (p) => {
  const t = tokens(p);
  return camposOrfaos(p).every(c => ![...tokens(c)].some(x => t.has(x)));
};
for (const [p, n] of porConsumo.parcial.sort((a, b) => b[1] - a[1]))
  console.log(`     ${String(n).padStart(5)}x  ${p.padEnd(26)} sem leitor: ${camposOrfaos(p).join(', ')}` +
    (suspeita(p) ? '   <- SOBRE-RECOLHA do mapa, nao do motor' : ''));
if (porConsumo.indeterminada.length)
  console.log(`  INDETERMINADAS (sem campo na tabela): ${String(porConsumo.indeterminada.length).padStart(3)} propriedades, ${soma(porConsumo.indeterminada)} declaracoes`);

// ── CASOS DE CONTROLO: a sonda confere-se a si mesma antes de imprimir ──────
//
// Esta sonda ja respondeu, com ar de numero, 12,9% em vez de 96,8% (uma
// reformatacao mudou a indentacao e a leitura dos bracos deixou de ver as
// entradas), 111 de 117 campos efetivos (a superficie dentro do fecho) e 142
// propriedades "sem campo na tabela" (a leitura da tabela partida). Nenhuma
// dessas corridas falhou: todas imprimiram um relatorio inteiro.
//
// O metodo que apanhou as quatro foi sempre o mesmo — conferir contra casos
// conhecidos ANTES de aceitar o numero. Isto e esse metodo dentro do
// instrumento, para nao depender de alguem se lembrar de o fazer.
//
// As afirmacoes sao de duas familias, e a distincao importa:
//   * INVARIANTES DE ESTRUTURA (quantos campos, que forma tem a reparticao) —
//     so quebram se a LEITURA partir.
//   * CASOS EFETIVOS conhecidos (`display`, `color`, `border-bottom-color`) —
//     propriedades que o motor le desde sempre em `layout.rs`. Se uma delas sair
//     de "efetiva", partiu a analise de consumo, nao o motor.
//
// NAO se afirma que alguma propriedade e GUARDADA. `font-style` era guardada de
// manha e foi ligada a tarde (`6eab4185`): um controlo desses transformaria uma
// melhoria do motor num erro da sonda, que e o oposto do que ela serve.
{
  const falhas = [];
  const diz = (cond, msg) => { if (!cond) falhas.push(msg); };

  diz(CAMPOS.size >= 80,
    `a tabela de props.rs deu so ${CAMPOS.size} campos — a leitura da tabela partiu`);
  diz(efetivos.size >= 40,
    `so ${efetivos.size} campos efetivos — o fecho a partir de quem desenha partiu`);
  for (const p of ['display', 'color', 'background-color', 'width', 'border-bottom-color'])
    diz(known.has(p) && consumo(p) === 'efetiva',
      `'${p}' devia ser EFETIVA e saiu '${known.has(p) ? consumo(p) : 'nao reconhecida'}'`);
  diz(known.has('content'),
    `'content' e servida por caminho proprio (stylesheet.rs) e deixou de ser lida`);
  diz(porConsumo.indeterminada.length <= 8,
    `${porConsumo.indeterminada.length} propriedades sem campo na tabela — eram 3; ` +
    `a ligacao nome->campo partiu`);
  diz(porConsumo.efetiva.length > porConsumo.guardada.length + porConsumo.indeterminada.length,
    `a reparticao inverteu-se (${porConsumo.efetiva.length} efetivas contra ` +
    `${porConsumo.guardada.length}+${porConsumo.indeterminada.length}) — sintoma da leitura partida`);

  if (falhas.length) {
    console.error('\n*** A SONDA NAO SE CONFERE — os numeros acima NAO valem ***');
    for (const f of falhas) console.error('  - ' + f);
    process.exitCode = 1;
  }
}
const nin = rows.filter(([p]) => !known.has(p) && inert.has(p));
console.log(`RECUSADAS com motivo (style/inert.rs): ${nin.length} propriedades, ${nin.reduce((a, [, n]) => a + n, 0)} declaracoes`);
const falta = rows.filter(([p]) => !known.has(p) && !inert.has(p));
console.log(`DESCONHECIDAS (a lista do que falta fazer): ${falta.length} propriedades, ${falta.reduce((a, [, n]) => a + n, 0)} declaracoes`);

// Ordenação primária por FOLHAS: ver o comentário do cabeçalho sobre porque a
// contagem de declarações sozinha deixa um autor decidir a lista de trabalho.
console.log('\nfolhas  decls  propriedade');
for (const [p, n] of falta.sort((a, b) => (folhas.get(b[0]) - folhas.get(a[0])) || (b[1] - a[1])))
  console.log(String(folhas.get(p)).padStart(6), String(n).padStart(6), ' ' + p);

console.log('');
console.log('GUARDADAS, por folhas — parseadas e sem consumidor:');
for (const [p, n] of porConsumo.guardada.sort((a, b) => (folhas.get(b[0]) - folhas.get(a[0])) || (b[1] - a[1])))
  console.log(String(folhas.get(p)).padStart(6), String(n).padStart(6), ' ' + p);
if (porConsumo.indeterminada.length) {
  console.log('');
  console.log('INDETERMINADAS — reconhecidas por caminho sem campo de tabela:');
  for (const [p, n] of porConsumo.indeterminada) console.log(String(n).padStart(6), ' ' + p);
}