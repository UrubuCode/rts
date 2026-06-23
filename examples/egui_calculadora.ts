// App de teste: uma CALCULADORA simples usando a GUI egui do RTS.
// Exercita a GUI de verdade: display, vários botões, estado, lógica.
//
// Rodar:  target/debug/rts.exe run examples/egui_calculadora.ts

import egui from "rts:egui";

class Window {
  __h: number;
  constructor(title: string, width: number, height: number) {
    this.__h = egui.openWindow(title, width, height, 0);
  }
  isOpen(): boolean { return egui.isOpen(this.__h) !== 0; }
  pump(): boolean { return egui.pump(this.__h) === 0; }
  beginFrame(): void { egui.beginFrame(this.__h); }
  endFrame(): void { egui.endFrame(this.__h); }
  label(text: string): void { egui.label(this.__h, text); }
  button(text: string): boolean { return egui.button(this.__h, text) !== 0; }
  rowBegin(): void { egui.horizontalBegin(this.__h); }
  rowEnd(): void { egui.horizontalEnd(this.__h); }
  close(): void { egui.close(this.__h); }
}

const win = new Window("Calculadora RTS", 260, 360);

// Estado da calculadora.
let display = "0";
let acumulador = 0.0;
let operacao = "";       // "", "+", "-", "*", "/"
let novoNumero = true;   // true quando o próximo dígito começa um número novo

// Converte o display (string) para número.
function displayParaNumero(s: string): number {
  return parseFloat(s);
}

// Anexa um dígito ao display.
function digito(d: string): void {
  if (novoNumero) {
    display = d;
    novoNumero = false;
  } else {
    if (display === "0") {
      display = d;
    } else {
      display = display + d;
    }
  }
}

// Aplica a operação pendente entre `acumulador` e o número atual do display.
function aplicar(): number {
  const atual = displayParaNumero(display);
  if (operacao === "+") return acumulador + atual;
  if (operacao === "-") return acumulador - atual;
  if (operacao === "*") return acumulador * atual;
  if (operacao === "/") {
    if (atual === 0.0) return 0.0;
    return acumulador / atual;
  }
  return atual;
}

// Define a operação (e calcula o resultado parcial).
function setOperacao(op: string): void {
  if (operacao !== "" && !novoNumero) {
    acumulador = aplicar();
    display = "" + acumulador;
  } else {
    acumulador = displayParaNumero(display);
  }
  operacao = op;
  novoNumero = true;
}

// Igual: calcula e zera a operação.
function igual(): void {
  if (operacao !== "") {
    acumulador = aplicar();
    display = "" + acumulador;
    operacao = "";
    novoNumero = true;
  }
}

// Limpa tudo.
function limpar(): void {
  display = "0";
  acumulador = 0.0;
  operacao = "";
  novoNumero = true;
}

while (win.isOpen()) {
  if (!win.pump()) break;
  win.beginFrame();

  // Display grande.
  win.label("");
  win.label("  " + display);
  win.label("");

  // Linha 1: 7 8 9 /
  win.rowBegin();
  if (win.button("  7  ")) digito("7");
  if (win.button("  8  ")) digito("8");
  if (win.button("  9  ")) digito("9");
  if (win.button("  /  ")) setOperacao("/");
  win.rowEnd();

  // Linha 2: 4 5 6 *
  win.rowBegin();
  if (win.button("  4  ")) digito("4");
  if (win.button("  5  ")) digito("5");
  if (win.button("  6  ")) digito("6");
  if (win.button("  *  ")) setOperacao("*");
  win.rowEnd();

  // Linha 3: 1 2 3 -
  win.rowBegin();
  if (win.button("  1  ")) digito("1");
  if (win.button("  2  ")) digito("2");
  if (win.button("  3  ")) digito("3");
  if (win.button("  -  ")) setOperacao("-");
  win.rowEnd();

  // Linha 4: 0 C = +
  win.rowBegin();
  if (win.button("  0  ")) digito("0");
  if (win.button("  C  ")) limpar();
  if (win.button("  =  ")) igual();
  if (win.button("  +  ")) setOperacao("+");
  win.rowEnd();

  win.endFrame();
}
win.close();
