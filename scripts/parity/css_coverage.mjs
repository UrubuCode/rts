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
// - RECONHECIDA: parseada e guardada.
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
for (const [f, ind] of [['parse.rs', 12], ['timing.rs', 8], ['vocab.rs', 8]]) {
  for (const line of fs.readFileSync(D + f, 'utf8').split('\n')) {
    const ind2 = line.length - line.trimStart().length;
    if (ind2 !== ind) continue;
    const m = line.trimStart().match(/^((?:"[a-zA-Z-]+"\s*\|\s*)*"[a-zA-Z-]+")\s*=>/);
    if (m) for (const q of m[1].match(/"[^"]+"/g)) { const nm = q.slice(1, -1); known.add(nm); if (f !== 'parse.rs') aliasable.add(nm); }
  }
}
// os prefixos de fornecedor que timing/vocab aceitam como alias.
// SO timing.rs e vocab.rs tiram o prefixo de fornecedor; parse.rs nao tira.
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
const nin = rows.filter(([p]) => !known.has(p) && inert.has(p));
console.log(`RECUSADAS com motivo (style/inert.rs): ${nin.length} propriedades, ${nin.reduce((a, [, n]) => a + n, 0)} declaracoes`);
const falta = rows.filter(([p]) => !known.has(p) && !inert.has(p));
console.log(`DESCONHECIDAS (a lista do que falta fazer): ${falta.length} propriedades, ${falta.reduce((a, [, n]) => a + n, 0)} declaracoes`);

// Ordenação primária por FOLHAS: ver o comentário do cabeçalho sobre porque a
// contagem de declarações sozinha deixa um autor decidir a lista de trabalho.
console.log('\nfolhas  decls  propriedade');
for (const [p, n] of falta.sort((a, b) => (folhas.get(b[0]) - folhas.get(a[0])) || (b[1] - a[1])))
  console.log(String(folhas.get(p)).padStart(6), String(n).padStart(6), ' ' + p);
