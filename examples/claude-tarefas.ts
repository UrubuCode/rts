// Uma pagina de tarefas COMPLETA — adicionar, editar, concluir e remover — a
// correr no motor RTS de ponta a ponta: o nosso parser de HTML, a nossa cascade
// de CSS, o nosso layout, o nosso motor de JavaScript e o egui a pintar.
//
// Nao usa React nem Preact de proposito. Aqueles provam que uma biblioteca de
// fora funciona aqui; esta prova o outro lado — o que o DOM oferece a quem
// escreve JavaScript directamente, que e onde se ve o que falta.
//
// O que ela exercita, e que ate hoje nao existia:
//
//   • `input.value` a LER o que foi digitado e a ESCREVER por cima (limpar o
//     campo depois de adicionar nao tinha forma nenhuma de se escrever);
//   • `el.focus()`, para o proximo caracter ir para o campo certo;
//   • `on<evento>` como propriedade e `el.click()`;
//   • identidade de no — cada linha guarda o seu proprio estado NO no, e
//     `event.target` tem de ser o mesmo objecto para o achar.
import egui from "rts:egui";
import dom from "rts:dom";
import { readFileSync } from "node:fs";

// -- O estado, e onde ele mora ---------------------------------------------
//
// Num array, e nao no DOM. O DOM e o que se VE; misturar os dois e como se
// descobre, a meio, que a fonte da verdade sao duas.
class Tarefa {
  texto: string;
  feita: boolean;
  constructor(texto: string, feita: boolean) {
    this.texto = texto;
    this.feita = feita;
  }
}

const doc = parseDocument(readFileSync("examples/claude-tarefas.html", "utf8"));
const lista: any = doc.getElementById("lista");
const campo: any = doc.getElementById("campo");
const botao: any = doc.getElementById("add");

const tarefas: Tarefa[] = [
  new Tarefa("compilar a pagina com escopo", true),
  new Tarefa("dar identidade aos nos", true),
  new Tarefa("ligar os formularios", false),
];

// Qual linha esta em EDICAO, por indice. `-1` e nenhuma.
let aEditar = -1;

// -- Pintar ----------------------------------------------------------------
//
// Reconstroi a lista inteira a cada mudanca. Nao e o que um reconciliador faz,
// e e o certo aqui: sao dez linhas, e um diff manual seria mais codigo do que
// aquilo que ele pouparia — mais uma segunda copia da verdade para desalinhar.
function pintar(): void {
  lista.innerHTML = "";
  if (tarefas.length === 0) {
    const vazio = doc.createElement("p");
    vazio.className = "vazio";
    vazio.textContent = "nada por fazer — escreve algo acima";
    lista.appendChild(vazio);
  }
  let i = 0;
  while (i < tarefas.length) {
    lista.appendChild(linhaDe(i, tarefas[i]));
    i = i + 1;
  }
  contar();
}

function linhaDe(i: number, t: Tarefa): any {
  const linha: any = doc.createElement("div");
  linha.className = t.feita ? "linha pronta" : "linha";

  const marca: any = doc.createElement("span");
  marca.className = "marca";
  marca.textContent = t.feita ? "x" : "o";
  // Clicar na marca alterna. O indice viaja no CLOSURE e nao num atributo:
  // um atributo obrigaria a reler e a converter uma string a cada clique.
  marca.onclick = function (): void {
    tarefas[i].feita = !tarefas[i].feita;
    aEditar = -1;
    pintar();
  };
  linha.appendChild(marca);

  if (aEditar === i) {
    // EM EDICAO: o texto da lugar a um campo com o valor la dentro, e o foco
    // vai para ele — que e a diferenca entre "editavel" e "editando".
    const campoLinha: any = doc.createElement("input");
    campoLinha.className = "campo";
    campoLinha.setAttribute("id", "edicao");
    campoLinha.value = t.texto;
    linha.appendChild(campoLinha);
    campoLinha.focus();

    const guardar: any = doc.createElement("span");
    guardar.className = "acao";
    guardar.textContent = "GUARDAR";
    guardar.onclick = function (): void {
      const novo = campoLinha.value;
      if (novo.length > 0) tarefas[i].texto = novo;
      aEditar = -1;
      pintar();
    };
    linha.appendChild(guardar);
  } else {
    const texto: any = doc.createElement("span");
    texto.className = "texto";
    texto.textContent = t.texto;
    linha.appendChild(texto);

    const editar: any = doc.createElement("span");
    editar.className = "acao";
    editar.textContent = "EDITAR";
    editar.onclick = function (): void {
      aEditar = i;
      pintar();
    };
    linha.appendChild(editar);
  }

  const apagar: any = doc.createElement("span");
  apagar.className = "acao perigo";
  apagar.textContent = "REMOVER";
  apagar.onclick = function (): void {
    tarefas.splice(i, 1);
    aEditar = -1;
    pintar();
  };
  linha.appendChild(apagar);
  return linha;
}

function contar(): void {
  let feitas = 0;
  let i = 0;
  while (i < tarefas.length) {
    if (tarefas[i].feita) feitas = feitas + 1;
    i = i + 1;
  }
  doc.getElementById("m-total").textContent = "" + tarefas.length;
  doc.getElementById("m-falta").textContent = "" + (tarefas.length - feitas);
  doc.getElementById("m-feito").textContent = "" + feitas;
}

// -- Adicionar -------------------------------------------------------------
//
// Ler o campo, limpa-lo e devolver-lhe o foco. As tres precisam do `value` nos
// dois sentidos, e e por isso que esta pagina nao podia existir ontem.
function adicionar(): void {
  const texto = campo.value;
  if (texto.length === 0) return;
  tarefas.push(new Tarefa(texto, false));
  campo.value = "";
  aEditar = -1;
  pintar();
  campo.focus();
}

botao.onclick = adicionar;
// Enter no campo tambem adiciona — e o que a mao espera fazer.
campo.onkeydown = function (e: any): void {
  if (e.key === "Enter") adicionar();
};

pintar();
campo.focus();

const win = egui.openWindow("RTS - Tarefas", 900, 760, 0);
while (egui.isOpen(win)) {
  if (!egui.pump(win)) break;
  egui.beginFrame(win);
  egui.render(win, doc._dom);
  egui.endFrame(win);
  // A ordem de um frame: teclado e edicao, depois cliques, depois timers.
  pumpInputEvents(doc);
  pumpEventCallbacks(doc);
  pumpTimerCallbacks(doc);
}
egui.close(win);
console.log("[fim] " + tarefas.length + " tarefas");
