// COBERTURA DE PROPRIEDADES CSS — quantas das que as folhas reais escrevem o
import fs from 'fs';
// motor reconhece, contadas por PROPRIEDADE e por DECLARAÇÃO.
//
//   node scripts/parity/css_coverage.mjs
//
// Existe para o número ser RE-MEDIDO em vez de citado. O que substituiu foi a
// linha "68 de 363 usadas" de `docs/ui/estado-motor-css.md`, que ninguém sabia
// reproduzir — um número sem instrumento é uma afirmação.
//
// ## Duas armadilhas de DENOMINADOR, e é por causa delas que isto é um ficheiro
//
// 1. `scripts/parity/pagina.combinada.html` JÁ EMBUTE `pagina.css`. Contar os
//    dois dá tudo a dobrar (dá para reconhecer: todos os totais saem PARES).
//    Aqui entra só a combinada.
// 2. O motor reconhece nomes por FORMA e não só por literal: `parse.rs` tem um
//    braço-guarda para as doze longhands `border-<lado>-<width|style|color>`, e
//    `style/radius.rs` e `style/logical.rs` reconhecem famílias inteiras. Uma
//    varredura só por literais declara-as em falta — e mandaria "implementar"
//    doze propriedades que já existem.
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
const D='E:/rts/crates/rts-dom/src/style/';
const known=new Set();
// braços literais do match, em parse.rs (12 espaços) e nos módulos novos (8).
const aliasable=new Set();
for(const [f,ind] of [['parse.rs',12],['timing.rs',8],['vocab.rs',8]]){
  for(const line of fs.readFileSync(D+f,'utf8').split('\n')){
    const ind2 = line.length - line.trimStart().length;
    if (ind2 !== ind) continue;
    const m = line.trimStart().match(/^((?:"[a-zA-Z-]+"\s*\|\s*)*"[a-zA-Z-]+")\s*=>/);
    if(m) for(const q of m[1].match(/"[^"]+"/g)) { const nm=q.slice(1,-1); known.add(nm); if(f!=='parse.rs') aliasable.add(nm); }
  }
}
// os prefixos de fornecedor que timing/vocab aceitam como alias.
// SO timing.rs e vocab.rs tiram o prefixo de fornecedor; parse.rs nao tira.
for(const p of aliasable) { known.add('-webkit-'+p); known.add('-moz-'+p); }
// borders::is_longhand (guarda por FORMA) e logical (tradução de eixo).
for(const s of ['top','right','bottom','left']) { known.add('border-'+s);
  for(const w of ['width','style','color']) known.add(`border-${s}-${w}`); }
for(const l of ['inline-start','inline-end','block-start','block-end']){
  known.add('inset-'+l); known.add('border-'+l);
  for(const w of ['width','style','color']) known.add(`border-${l}-${w}`); }
for(const x of ['inset','inset-inline','inset-block']) known.add(x);
// style/radius.rs: reconhece pela FORMA border-<canto>-radius.
for(const c of ['top-left','top-right','bottom-right','bottom-left','start-start','start-end','end-end','end-start']) known.add('border-'+c+'-radius');
// as recusadas explicitamente (style/inert.rs), contadas a PARTE.
const inert=new Set();
{ const t=fs.readFileSync(D+'inert.rs','utf8');
  const body=t.slice(t.indexOf('matches!('));
  for(const m of body.matchAll(/"([a-zA-Z-]+)"/g)) { inert.add(m[1]); inert.add('-webkit-'+m[1]); inert.add('-moz-'+m[1]); } }
function cssOf(p){let t=fs.readFileSync(p,'utf8');
  if(p.endsWith('.html')){let c='';for(const m of t.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/gi))c+=m[1]+'\n';t=c;}
  return t.replace(/\/\*[\s\S]*?\*\//g,'');}
function scan(css){const d=new Map();let depth=0,buf='';const bl=[];
  for(let i=0;i<css.length;i++){const c=css[i];
    if(c==='{'){depth++;if(depth===1){buf='';continue;}}
    if(c==='}'){depth--;if(depth===0){bl.push(buf);buf='';continue;}}
    if(depth>=1)buf+=c;}
  for(const b of bl)for(const s of b.split(';')){const i=s.indexOf(':');if(i<0)continue;
    const p=s.slice(0,i).trim().toLowerCase();
    if(!/^-?[a-z][a-z0-9-]*$/.test(p)||p.startsWith('--'))continue;
    d.set(p,(d.get(p)||0)+1);} return d;}
const srcs={mediawiki:'E:/rts/scripts/parity/pagina.combinada.html',google:'E:/rts/google.css',wa:'E:/rts/wa.css',waapp:'E:/rts/wa-app.css'};
const uni=new Map();
for(const [k,p] of Object.entries(srcs)){const d=scan(cssOf(p));
  let dn=0,dk=0; for(const [prop,n] of d){dn+=n; if(known.has(prop))dk+=n; uni.set(prop,(uni.get(prop)||0)+n);}
  console.log(k.padEnd(10),'props',String(d.size).padStart(4),'reconhecidas',String([...d.keys()].filter(x=>known.has(x)).length).padStart(4),
    ' declaracoes',String(dn).padStart(6),'cobertas',String(dk).padStart(6),(100*dk/dn).toFixed(1)+'%');}
const desc=new Set(['syntax','inherits','src','unicode-range','symbols','suffix','speak','system','additive-symbols','negative','pad','range','fallback','prefix','font-display']);
const rows=[...uni].filter(([p])=>!desc.has(p));
const tot=rows.length, rec=rows.filter(([p])=>known.has(p)).length;
const dn=rows.reduce((a,[,n])=>a+n,0), dk=rows.filter(([p])=>known.has(p)).reduce((a,[,n])=>a+n,0);
console.log(`\nUNIAO (sem descritores de at-rule): ${rec}/${tot} propriedades (${(100*rec/tot).toFixed(1)}%), ${dk}/${dn} declaracoes (${(100*dk/dn).toFixed(1)}%)`);
const nin = rows.filter(([p]) => !known.has(p) && inert.has(p));
console.log(`RECUSADAS com motivo (style/inert.rs): ${nin.length} propriedades, ${nin.reduce((a,[,n])=>a+n,0)} declaracoes`);
const falta = rows.filter(([p]) => !known.has(p) && !inert.has(p));
console.log(`DESCONHECIDAS (a lista do que falta fazer): ${falta.length} propriedades, ${falta.reduce((a,[,n])=>a+n,0)} declaracoes`);
console.log('');
for(const [p,n] of falta.sort((a,b)=>b[1]-a[1])) console.log(String(n).padStart(6),p);
