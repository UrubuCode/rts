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
// braços literais do match, em parse.rs (12 espaços) e nos módulos novos (8).
const aliasable = new Set();
// O terceiro campo diz se o módulo TIRA o prefixo de fornecedor antes de casar.
// Era deduzido de `f !== 'parse.rs'`, o que valia enquanto os módulos ligados
// por `_ if` fossem só o `timing` e o `vocab` — que tiram. `grid_lines` não
// tira (nenhuma folha escreve `-webkit-grid-column`), e herdar o `true` por ser
// um módulo faria a sonda dar por reconhecidos seis nomes prefixados que o
// motor recusa. É a mesma classe de erro que o `-ms-` em falta no `inert`:
// a sonda a afirmar uma capacidade em vez de a ler.
for (const [f, ind, prefixado] of [
  ['parse.rs', 12, false],
  ['timing.rs', 8, true],
  ['vocab.rs', 8, true],
  ['grid_lines.rs', 8, false],
]) {
  for (const line of fs.readFileSync(D + f, 'utf8').split('\n')) {
    const ind2 = line.length - line.trimStart().length;
    if (ind2 !== ind) continue;
    const m = line.trimStart().match(/^((?:"[a-zA-Z-]+"\s*\|\s*)*"[a-zA-Z-]+")\s*=>/);
    if (m) for (const q of m[1].match(/"[^"]+"/g)) { const nm = q.slice(1, -1); known.add(nm); if (prefixado) aliasable.add(nm); }
  }
}
for (const p of aliasable) { known.add('-webkit-' + p); known.add('-moz-' + p); }
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
for (const [f, ind] of [['parse.rs', 12], ['timing.rs', 8], ['vocab.rs', 8], ['grid_lines.rs', 8]]) {
  const linhas = fs.readFileSync(D + f, 'utf8').split('\n');
  for (let i = 0; i < linhas.length; i++) {
    const l = linhas[i];
    if (l.length - l.trimStart().length !== ind) continue;
    const m = l.trimStart().match(/^((?:"[a-zA-Z-]+"\s*\|\s*)*"[a-zA-Z-]+")\s*=>/);
    if (!m) continue;
    // o CORPO do braço: até à linha seguinte com a mesma indentação.
    let corpo = l;
    for (let j = i + 1; j < linhas.length; j++) {
      const lj = linhas[j];
      if (lj.trim() && lj.length - lj.trimStart().length <= ind) break;
      corpo += '\n' + lj;
    }
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
    for (const q of m[1].match(/"[^"]+"/g)) {
      const nm = q.slice(1, -1);
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
  ['logical.rs', ['inset', 'inset-inline', 'inset-block', 'inset-inline-start', 'inset-inline-end', 'inset-block-start', 'inset-block-end']],
]) {
  const t = fs.readFileSync(D + mod, 'utf8');
  const escreve = new Set([...t.matchAll(/css\.([a-z_0-9]+)/g)].map(x => x[1]).filter(c => CAMPOS.has(c)));
  for (const nm of nomes) {
    if (!camposDoNome.has(nm)) camposDoNome.set(nm, new Set());
    for (const c of escreve) camposDoNome.get(nm).add(c);
  }
}
// Prefixos de fornecedor: o mesmo campo do nome nu.
for (const p of aliasable)
  for (const pre of ['-webkit-', '-moz-'])
    if (camposDoNome.has(p)) camposDoNome.set(pre + p, camposDoNome.get(p));

/** 'efetiva' | 'guardada' | 'indeterminada' (sem campo de tabela conhecido). */
function consumo(prop) {
  const cs = camposDoNome.get(prop);
  if (!cs || cs.size === 0) return 'indeterminada';
  return [...cs].some(c => efetivos.has(c)) ? 'efetiva' : 'guardada';
}

// as recusadas explicitamente (style/inert.rs), contadas a PARTE.
const inert = new Set();
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
const porConsumo = { efetiva: [], guardada: [], indeterminada: [] };
for (const r of conhecidas) porConsumo[consumo(r[0])].push(r);
const soma = a => a.reduce((x, [, n]) => x + n, 0);
console.log(`  EFETIVAS  (lidas por quem desenha):   ${String(porConsumo.efetiva.length).padStart(3)} propriedades, ${soma(porConsumo.efetiva)} declaracoes`);
console.log(`  GUARDADAS (parseadas, ninguem as le): ${String(porConsumo.guardada.length).padStart(3)} propriedades, ${soma(porConsumo.guardada)} declaracoes`);
if (porConsumo.indeterminada.length)
  console.log(`  INDETERMINADAS (sem campo na tabela): ${String(porConsumo.indeterminada.length).padStart(3)} propriedades, ${soma(porConsumo.indeterminada)} declaracoes`);
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