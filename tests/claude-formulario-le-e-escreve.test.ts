import { test, expect } from "rts:test";

// O que um formulario precisa, e que nao existia ate hoje.
//
// Nao e uma lista de conveniencias: sem estas quatro nao ha pagina que aceite
// texto. Uma delas — a ESCRITA — nem sequer tinha caminho no Rust: o vocabulario
// do DOM era alimentar tecla a tecla, e `el.value = ""` (limpar o campo depois
// de submeter) nao se escreve com isso.

const doc = parseDocument("<input id='i' value='do html'><button id='b'>x</button>");
const campo: any = doc.getElementById("i");
const botao: any = doc.getElementById("b");

// O `getAttribute("value")` da o valor INICIAL do HTML e nunca muda. O que o
// utilizador digitou vive noutro sitio, e era ali que ninguem chegava.
test("`value` le o valor editado, nao o atributo", function () {
  expect(campo.value).toBe("do html");
  expect(campo.getAttribute("value")).toBe("do html");
});

test("`value` escreve por cima, e aceita a string vazia", function () {
  campo.value = "escrito pelo programa";
  expect(campo.value).toBe("escrito pelo programa");
  campo.value = "";
  expect(campo.value).toBe("");
});

// O foco e do DOCUMENTO: decide para onde vai a proxima tecla. Um `blur` num
// elemento que nao o tem nao pode desfocar outro.
test("`focus` e `blur` movem o foco do documento", function () {
  campo.focus();
  botao.blur();
  campo.blur();
  expect(campo.value).toBe("");
});

// `el.click()` dispara com bubbling, e `onclick` como propriedade SUBSTITUI em
// vez de acumular — ao contrario de dois `addEventListener`.
test("`onclick` substitui, e `click()` dispara", function () {
  let a = 0;
  let b = 0;
  botao.onclick = function () { a = a + 1; };
  botao.onclick = function () { b = b + 1; };
  botao.click();
  expect(a).toBe(0);
  expect(b).toBe(1);
});
