// Cross-runtime: UMA coisa — closure capturada dentro de bloco aninhado captura
// o binding SHADOWING (o mais interno visível no ponto de criação), não o de fora.
// Variações: shadow em bloco nu, if/else, shadow do mesmo nome em 3 níveis,
// shadow que some ao sair do bloco, var vs let no shadow, shadow de parâmetro,
// e closure criada ANTES do shadow (captura a de fora).

// 1) bloco nu: closure interna vê a interna; a externa continua vendo a externa
let v = "outer";
const readOuter = () => v;
let readInner: () => string;
{
  let v = "inner";
  readInner = () => v;
  v = "inner2";
}
console.log("outer=" + readOuter() + " inner=" + readInner());

// 2) três níveis do mesmo nome
let n = 1;
const l0 = () => n;
let l1: () => number;
let l2: () => number;
{
  let n = 2;
  l1 = () => n;
  {
    let n = 3;
    l2 = () => n;
  }
}
console.log("levels=" + l0() + "," + l1() + "," + l2());

// 3) if/else: cada braço tem seu próprio binding
let picked: () => string;
const flag = true;
if (flag) {
  let side = "then";
  picked = () => side;
} else {
  let side = "else";
  picked = () => side;
}
console.log("branch=" + picked());

// 4) var NÃO faz shadow em bloco — é function-scoped, mesmo binding
function varBlock(): string {
  var w = "fn";
  const read = () => w;
  {
    var w2 = "block";
    w = "reassigned";
  }
  return read() + ":" + w2;
}
console.log("var_no_block_shadow=" + varBlock());

// 5) let em bloco faz shadow de var da função
function letShadowsVar(): string {
  var s = "var";
  const outerRead = () => s;
  let innerRead: () => string;
  {
    let s = "let";
    innerRead = () => s;
  }
  return outerRead() + ":" + innerRead();
}
console.log("let_shadows_var=" + letShadowsVar());

// 6) shadow de PARÂMETRO dentro do corpo
function shadowParam(p: string): string {
  const readParam = () => p;
  let readLocal: () => string;
  {
    let p = "shadowed";
    readLocal = () => p;
  }
  return readParam() + ":" + readLocal();
}
console.log("shadow_param=" + shadowParam("arg"));

// 7) closure criada ANTES do bloco de shadow captura a de fora, mesmo chamada depois
let t = "before";
const early = () => t;
{
  let t = "shadow";
  void t;
}
t = "mutated";
console.log("created_before_shadow=" + early());

// 8) bloco dentro de loop: shadow + binding por-iteração combinam
const fns: Array<() => string> = [];
let outerName = "O";
for (let i = 0; i < 2; i++) {
  {
    let outerName = "I" + i;
    fns.push(() => outerName);
  }
}
console.log("loop_block_shadow=" + fns[0]() + "," + fns[1]() + " outer=" + outerName);
