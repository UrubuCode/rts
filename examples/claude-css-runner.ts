// O CORREDOR do corpus de fixtures CSS: corre `tests/css/*.html` pelo nosso
// motor (`rts:dom`) e compara com o que o Chrome mediu.
//
//   target/release/examples/run_fixture.exe examples/claude-css-runner.ts
//   CSS_TOL=2 ...run_fixture.exe examples/claude-css-runner.ts      # 2px
//   CSS_FILTRO=flex ...run_fixture.exe examples/claude-css-runner.ts
//   CSS_VERBOSE=1 ...                                               # tudo
//
// O Chrome é a RÉGUA. O `.esperado.json` ao lado de cada fixture foi MEDIDO
// nele a 1280x800 (`scripts/css_fixtures_medir.md` diz como), nunca escrito à
// mão — o valor deste corpus está exatamente aí. Ver `tests/css/README.md`.
import { readFileSync, readdirSync } from "node:fs";
import {
  parseHtml, free, querySelectorAllCount, querySelectorAllAt,
  getAttribute, boundingRect, computedProperty,
} from "rts:dom";

const PASTA = "tests/css";
const tolerancia = Number((process as any).env.CSS_TOL ?? "1");
const filtro = String((process as any).env.CSS_FILTRO ?? "");
const verboso = String((process as any).env.CSS_VERBOSE ?? "") !== "";

// O DENOMINADOR é o número de ficheiros `.html` que existem, e não o das que
// chegaram a correr: uma fixture sem esperado, ou que rebentou a leitura, tem
// de aparecer no relatório em vez de sair discretamente da conta. É a regra
// "verifique a entrada, não só a saída" do CLAUDE.md aplicada a este corpus.
const fixtures: string[] = [];
for (const nome of readdirSync(PASTA) as string[]) {
  if (!nome.endsWith(".html")) { continue; }
  if (filtro.length > 0 && nome.indexOf(filtro) < 0) { continue; }
  fixtures.push(nome);
}
fixtures.sort();

interface Desvio { fixture: string; onde: string; esperado: string; obtido: string; }

const desvios: Desvio[] = [];
const semEsperado: string[] = [];
const passam: string[] = [];
const falham: string[] = [];

// Uma propriedade de estilo só é comparada quando a fixture PEDE, por
//
//   <meta name="fixar-estilo" content="color,background-color">
//
// e não por a mencionar algures no CSS. A diferença é grande: o nosso
// `computedProperty` devolve `""` para uma propriedade não declarada (não
// resolve o valor inicial), então comparar todas as 23 em todos os elementos
// enche o relatório com centenas de "esperado block → obtido """ e afoga a
// geometria. Esse `""` continua a ser um desvio real e as fixtures que o
// fixam declaram-no à mesma; o que o `meta` evita é medi-lo 300 vezes de
// caminho para outra coisa. Está escrito no README como o que é: um
// estreitamento deliberado do que se compara, não um ajuste do esperado.
function lista(fonte: string, nomeDoMeta: string): string[] {
  const marca = fonte.indexOf("name=\"" + nomeDoMeta + "\"");
  if (marca < 0) { return []; }
  const c = fonte.indexOf("content=\"", marca);
  if (c < 0) { return []; }
  const fim = fonte.indexOf("\"", c + 9);
  if (fim < 0) { return []; }
  const saida: string[] = [];
  for (const p of fonte.substring(c + 9, fim).split(",")) {
    const t = p.trim();
    if (t.length > 0) { saida.push(t); }
  }
  return saida;
}

function proximo(a: number, b: number): boolean {
  const d = a - b;
  return (d < 0 ? -d : d) <= tolerancia;
}

function arredondar(v: number): number {
  return Math.round(v * 100) / 100;
}

for (const nome of fixtures) {
  const base = nome.substring(0, nome.length - 5);
  const caminhoEsperado = PASTA + "/" + base + ".esperado.json";

  let esperado: any = null;
  try {
    esperado = JSON.parse(readFileSync(caminhoEsperado, "utf8") as string);
  } catch (e) {
    semEsperado.push(nome);
    continue;
  }

  const fonte = readFileSync(PASTA + "/" + nome, "utf8") as string;
  const doc = parseHtml(fonte);

  // Os ids que o nosso motor encontrou, para o esperado poder acusar um que
  // não exista de todo — um seletor que não casa e uma caixa errada são coisas
  // diferentes e o relatório tem de as separar.
  const nosso: any = {};
  const total = querySelectorAllCount(doc, "*");
  for (let i = 0; i < total; i = i + 1) {
    const n = querySelectorAllAt(doc, "*", i);
    const id = getAttribute(doc, n, "id") as string;
    if (id.length === 0) { continue; }
    nosso[id] = n;
  }

  const antes = desvios.length;
  const elementos = esperado.elementos;
  const ids: string[] = Object.keys(elementos);
  const propsAqui = lista(fonte, "fixar-estilo");
  const idsDeEstilo = lista(fonte, "fixar-estilo-em");

  for (const id of ids) {
    const alvo = nosso[id];
    if (alvo === undefined) {
      desvios.push({ fixture: nome, onde: "#" + id, esperado: "o elemento existe", obtido: "o motor não o encontrou" });
      continue;
    }
    const r: number[] = elementos[id].rect;
    const meu = [
      arredondar(boundingRect(doc, alvo, 0) as number),
      arredondar(boundingRect(doc, alvo, 1) as number),
      arredondar(boundingRect(doc, alvo, 2) as number),
      arredondar(boundingRect(doc, alvo, 3) as number),
    ];
    const rotulos = ["x", "y", "w", "h"];
    for (let k = 0; k < 4; k = k + 1) {
      if (!proximo(meu[k], r[k])) {
        desvios.push({
          fixture: nome, onde: "#" + id + "." + rotulos[k],
          esperado: String(r[k]), obtido: String(meu[k]),
        });
      }
    }
    // `fixar-estilo-em` limita a comparação de estilo aos elementos que a
    // fixture nomeia. Sem ele, comparam-se todos.
    if (idsDeEstilo.length > 0 && idsDeEstilo.indexOf(id) < 0) { continue; }
    for (const p of propsAqui) {
      if (elementos[id].estilo[p] === undefined) {
        // A fixture pede uma propriedade que a medição no Chrome não recolheu.
        // É uma falha do INSTRUMENTO e não do motor, e tem de se ver como tal.
        desvios.push({ fixture: nome, onde: "#" + id + " {" + p + "}",
                       esperado: "medido no Chrome", obtido: "ausente do .esperado.json" });
        continue;
      }
      const querido = String(elementos[id].estilo[p]);
      const obtido = String(computedProperty(doc, alvo, p));
      if (obtido !== querido) {
        desvios.push({ fixture: nome, onde: "#" + id + " {" + p + "}", esperado: querido, obtido: obtido });
      }
    }
  }

  free(doc);
  if (desvios.length === antes) { passam.push(nome); } else { falham.push(nome); }
}

const existentes = fixtures.length;
console.log("");
console.log("corpus CSS — " + String(passam.length) + "/" + String(existentes) +
            " passam  (tolerância " + String(tolerancia) + "px, régua: Chrome 1280x800)");
if (semEsperado.length > 0) {
  console.log("SEM ESPERADO (contam no denominador e não passam): " + semEsperado.join(", "));
}
console.log("");

let atual = "";
for (const d of desvios) {
  if (d.fixture !== atual) {
    atual = d.fixture;
    let n = 0;
    for (const o of desvios) { if (o.fixture === atual) { n = n + 1; } }
    console.log("  " + atual + "  (" + String(n) + " desvios)");
  }
  console.log("    " + d.onde + "  esperado " + d.esperado + "  →  obtido " + d.obtido);
}
if (desvios.length > 0) { console.log(""); }

console.log("passam: " + String(passam.length) +
            " | falham: " + String(falham.length + semEsperado.length) +
            " | desvios: " + String(desvios.length));
if (verboso) {
  for (const n of passam) { console.log("  ok  " + n); }
}
