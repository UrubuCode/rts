// Cross-runtime: um PARÂMETRO que faz sombra a um nome do topo do módulo, lido
// pela primeira vez dentro de um laço que está dentro de um `try`.
//
// O motor guarda um parâmetro capturado numa environment sob uma chave cunhada
// da SOLETRAÇÃO do nome — `CachedSet { key: Key(0), value: v0 }` no IR — e a
// leitura dentro do laço protegido resolveu com um `hops` a mais, aterrando na
// environment do MÓDULO. Como o módulo tem um `c` também, e a chave é a mesma,
// a leitura respondeu o slot errado: `undefined` onde todo o runtime responde
// o argumento.
//
// São quatro condições ao mesmo tempo, e cada uma sozinha passa — por isso os
// controlos estão aqui e não noutro ficheiro. Tirar o `try`, tirar o laço, ler
// o parâmetro uma vez antes do laço, ou renomear qualquer um dos dois lados
// faz o defeito desaparecer. É também por isso que `obfuscated/` nunca o ia
// encontrar: um ofuscador RENOMEIA tudo, que é exactamente o que o cura.
//
// `emit/protect.rs` já documenta e corrige esta classe para o binding de um
// `catch` — o caso 8 abaixo é o que fixa essa correcção — e o corpo de um laço
// dentro da região ficou de fora.

function marca(v: unknown): string {
  return v === undefined ? "UNDEF" : "ok";
}

// --- os quatro factores juntos: é aqui que o motor respondia UNDEF ---

function comFor(c: object): string {
  try {
    let visto = "nunca";
    for (let w = 0; w < 1; w++) visto = marca(c);
    return visto;
  } catch (e) {
    return "THREW " + String(e);
  }
}

function comWhile(c: object): string {
  try {
    let visto = "nunca";
    let n = 0;
    while (n < 1) {
      n = n + 1;
      visto = marca(c);
    }
    return visto;
  } catch (e) {
    return "THREW " + String(e);
  }
}

function comForOf(c: object): string {
  try {
    let visto = "nunca";
    for (const _ of [0]) visto = marca(c);
    return visto;
  } catch (e) {
    return "THREW " + String(e);
  }
}

function comForIn(c: object): string {
  try {
    let visto = "nunca";
    for (const _ in { a: 1 }) visto = marca(c);
    return visto;
  } catch (e) {
    return "THREW " + String(e);
  }
}

// O laço aninhado dentro de outro laço, ainda sob o mesmo `try`: se a correcção
// empurrar uma environment só para o laço de fora, este continua errado.
function comForAninhado(c: object): string {
  try {
    let visto = "nunca";
    for (let a = 0; a < 1; a++) {
      for (let b = 0; b < 1; b++) visto = marca(c);
    }
    return visto;
  } catch (e) {
    return "THREW " + String(e);
  }
}

// O laço dentro do CATCH, não do corpo protegido.
function noCatch(c: object): string {
  try {
    throw new Error("x");
  } catch (e) {
    let visto = "nunca";
    for (let w = 0; w < 1; w++) visto = marca(c);
    return visto;
  }
}

// O laço dentro do FINALLY.
function noFinally(c: object): string {
  let visto = "nunca";
  try {
    // nada
  } finally {
    for (let w = 0; w < 1; w++) visto = marca(c);
  }
  return visto;
}

// --- controlos: cada um isola um dos quatro factores ---

// (1) sem `try`
function semTry(c: object): string {
  let visto = "nunca";
  for (let w = 0; w < 1; w++) visto = marca(c);
  return visto;
}

// (2) sem laço
function semLaco(c: object): string {
  try {
    return marca(c);
  } catch (e) {
    return "THREW " + String(e);
  }
}

// (3) o parâmetro lido ANTES do laço
function lidoAntes(c: object): string {
  try {
    const primeiro = marca(c);
    let visto = "nunca";
    for (let w = 0; w < 1; w++) visto = marca(c);
    return primeiro + "/" + visto;
  } catch (e) {
    return "THREW " + String(e);
  }
}

// (4) o parâmetro com um nome que o módulo NÃO tem
function semColisao(z: object): string {
  try {
    let visto = "nunca";
    for (let w = 0; w < 1; w++) visto = marca(z);
    return visto;
  } catch (e) {
    return "THREW " + String(e);
  }
}

// --- a sombra tem de continuar a ser uma sombra ---

// O parâmetro é o que se lê, não o nome do módulo: se a correcção resolver o
// UNDEF ligando o nome à environment do módulo, este caso apanha-a, porque a
// resposta passaria a ser o valor de fora em vez do argumento.
const etiqueta = "MODULO";

function qualDosDois(etiqueta: string): string {
  try {
    let visto = "nunca";
    for (let w = 0; w < 1; w++) visto = etiqueta;
    return visto;
  } catch (e) {
    return "THREW " + String(e);
  }
}

// O binding de um `catch` a fazer sombra ao mesmo nome, dentro de um laço: é a
// correcção que `emit/protect.rs` já tem, e fica pinada aqui para não sair sem
// se dar por isso.
const e = "MODULO";

function catchSombraDentroDeLaco(): string {
  let visto = "nunca";
  try {
    throw "APANHADO";
  } catch (e) {
    for (let w = 0; w < 1; w++) visto = e as string;
  }
  return visto + "/" + e;
}

// --- e o mesmo com um parâmetro que uma CLOSURE também lê ---

function comClosure(c: object): string {
  const ler = (): string => marca(c);
  try {
    let visto = "nunca";
    for (let w = 0; w < 1; w++) visto = ler();
    return visto;
  } catch (err) {
    return "THREW " + String(err);
  }
}

// O nome do módulo que colide. `c` é o parâmetro de quase toda a gente acima, e
// ter aqui um `c` de topo é a primeira das quatro condições.
const CASOS: object[] = [{ v: 1 }];

for (const c of CASOS) {
  console.log("comFor          ", comFor(c));
  console.log("comWhile        ", comWhile(c));
  console.log("comForOf        ", comForOf(c));
  console.log("comForIn        ", comForIn(c));
  console.log("comForAninhado  ", comForAninhado(c));
  console.log("noCatch         ", noCatch(c));
  console.log("noFinally       ", noFinally(c));
  console.log("semTry          ", semTry(c));
  console.log("semLaco         ", semLaco(c));
  console.log("lidoAntes       ", lidoAntes(c));
  console.log("semColisao      ", semColisao(c));
  console.log("comClosure      ", comClosure(c));
}

// A mesma chamada, com o argumento vindo de um índice em vez do `for`-`of`: a
// origem do argumento não é um dos quatro factores, e isto é o que o diz.
console.log("porIndice       ", comFor(CASOS[0]));

console.log("qualDosDois     ", qualDosDois("PARAMETRO"));
console.log("etiqueta        ", etiqueta);
console.log("catchSombra     ", catchSombraDentroDeLaco());
