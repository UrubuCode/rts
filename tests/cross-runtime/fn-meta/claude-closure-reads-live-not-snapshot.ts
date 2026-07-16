// Cross-runtime: UMA coisa — closure captura o BINDING, não uma cópia do valor.
// Mutar a variável DEPOIS de criar a closure muda o que a closure lê. Distinto de
// claude-closure-mutates-captured (lá a closure é quem muta; aqui quem muta é o
// escopo de fora, e a closure só observa). Variações: let externo, param de fn,
// reatribuição de função, hoisting de var, closure criada antes vs depois, e o
// contraste com const-snapshot.

// 1) let no escopo de módulo, mutado depois da criação da closure
let x = 1;
const readX = () => x;
console.log("before=" + readX());
x = 2;
console.log("after=" + readX());
x = x + 40;
console.log("after2=" + readX());

// 2) snapshot explícito via const NÃO acompanha
let y = 1;
const snap = y;
const readSnap = () => snap;
const readLive = () => y;
y = 99;
console.log("snap=" + readSnap() + " live=" + readLive());

// 3) closure sobre parâmetro de função, mutado depois pelo próprio corpo
function paramLive(p: number) {
  const read = () => p;
  p = p * 3;
  return read;
}
console.log("param_mutated_after=" + paramLive(5)());

// 4) duas closures criadas em MOMENTOS diferentes veem o mesmo binding atual
let z = 0;
const early = () => z;
z = 7;
const late = () => z;
z = 8;
console.log("early=" + early() + " late=" + late());

// 5) var hoisted: closure criada antes da atribuição lê o valor atual na chamada
function hoisted() {
  const read = () => w;
  var w = 3;
  const first = read();
  w = 4;
  return first + ":" + read();
}
console.log("var_hoisted=" + hoisted());

// 6) reatribuir a VARIÁVEL que guarda a função não muda a closure já capturada
let impl = () => "first";
const callImpl = () => impl();
const captured = impl;
impl = () => "second";
console.log("via_binding=" + callImpl() + " via_capture=" + captured());

// 7) closure lê valor atual mesmo através de várias mutações intercaladas
let acc = "";
let tick = 0;
const record = () => {
  acc += tick;
};
record();
tick = 1;
record();
tick = 2;
record();
console.log("interleaved=" + acc);

// 8) objeto capturado: mutar o CONTEÚDO vs reatribuir o BINDING
let box = { n: 1 };
const readBox = () => box.n;
box.n = 2;
console.log("mutate_content=" + readBox());
box = { n: 3 };
console.log("rebind=" + readBox());
