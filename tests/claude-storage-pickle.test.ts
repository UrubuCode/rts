import { describe, test, expect } from "rts:test";
import { fs } from "rts";

// `Storage` (Web Storage API) em RUST, persistindo como PICKLE.
//
// Duas coisas se provam aqui.
//
// 1. A API da spec: getItem/setItem/removeItem/clear/key/length, string-only,
//    `null` para chave ausente.
//
// 2. A PERSISTÊNCIA entre execuções, gravada como pickle (`rts_engine::heap::
//    pickle`) em vez de texto. Pickle porque guarda estrutura e tipo — um valor
//    com `\n` ou com o separador dentro quebraria um "k=v por linha", e isso é
//    conteúdo comum —, porque é o mesmo mecanismo do resto do projeto, e porque
//    produz um bloco opaco de bytes, que é o que se cifra quando a criptografia
//    entrar.
//
// Contexto de POR QUE em Rust: a primeira versão era um prelude `.ts`, e as
// ~68 linhas que ela acrescentava faziam `tests/node_util_parseargs.test.ts` —
// sem relação nenhuma — TRAVAR. Bisseção binária isolou o gatilho em dois
// arrays globais VAZIOS no prelude (estado global de módulo vira gcell). Em
// Rust o estado mora na struct/HandleTable e a classe de bug não existe.
//
// Pré-computado no top-level (regra do projeto: método dentro de test() pode
// perder handle pro GC).

const BASE = "target/tmp-claude-storage";
if (!fs.is_dir(BASE)) { fs.create_dir_all(BASE); }

// ── 1. API em memória (sem persistTo) ───────────────────────────────────────
const mem = new Storage();
const memVazio = mem.length;
mem.setItem("a", "1");
mem.setItem("b", "2");
const memLen = mem.length;
const memA = mem.getItem("a");
const memAusente = mem.getItem("naoexiste");
const memKey0 = mem.key(0);
const memKeyFora = mem.key(9);
// sobrescrever NÃO duplica a chave
mem.setItem("a", "9");
const memSobrescrito = mem.getItem("a");
const memLenAposSobrescrever = mem.length;

// ── 2. remove e clear ───────────────────────────────────────────────────────
const mut = new Storage();
mut.setItem("x", "1");
mut.setItem("y", "2");
mut.removeItem("x");
const aposRemove = mut.length;
const xRemovido = mut.getItem("x");
mut.removeItem("naoexiste"); // no-op, não pode explodir
const aposRemoveAusente = mut.length;
mut.clear();
const aposClear = mut.length;

// ── 3. PERSISTE entre "execuções" (dois Storage, mesmo arquivo) ────────────
const P1 = BASE + "/basico.pickle";
if (fs.exists(P1)) { fs.remove_file(P1); }
const escreve = new Storage();
escreve.persistTo(P1);
escreve.setItem("user", "ana");
escreve.setItem("visitas", "3");
const arquivoCriado = fs.exists(P1);
// outra instância, mesmo arquivo: é o que uma segunda execução veria
const le = new Storage();
le.persistTo(P1);
const leUser = le.getItem("user");
const leVisitas = le.getItem("visitas");
const leLen = le.length;
const leOrdem = le.key(0);

// ── 4. valor HOSTIL — o que um formato de texto quebraria ──────────────────
const P2 = BASE + "/hostil.pickle";
if (fs.exists(P2)) { fs.remove_file(P2); }
const hostil = "linha1\nlinha2=x\ty|z\r\nfim";
const gravaHostil = new Storage();
gravaHostil.persistTo(P2);
gravaHostil.setItem("bruto", hostil);
const leHostil = new Storage();
leHostil.persistTo(P2);
const voltouHostil = leHostil.getItem("bruto");

// ── 5. remove/clear TAMBÉM persistem ───────────────────────────────────────
const P3 = BASE + "/mutacao.pickle";
if (fs.exists(P3)) { fs.remove_file(P3); }
const m1 = new Storage();
m1.persistTo(P3);
m1.setItem("a", "1");
m1.setItem("b", "2");
m1.removeItem("a");
const m2 = new Storage();
m2.persistTo(P3);
const persistidoAposRemove = m2.length;
const aChaveSumiu = m2.getItem("a");
m2.clear();
const m3 = new Storage();
m3.persistTo(P3);
const persistidoAposClear = m3.length;

// ── 6. arquivo CORROMPIDO não derruba — storage segue utilizável ───────────
const P4 = BASE + "/corrompido.pickle";
fs.write(P4, "isto nao e um pickle valido {{{");
const corrompido = new Storage();
corrompido.persistTo(P4);
const corrompidoLen = corrompido.length;
corrompido.setItem("novo", "ok"); // ainda funciona depois do arquivo ruim
const corrompidoUsavel = corrompido.getItem("novo");

// ── 7. arquivo INEXISTENTE é só um storage vazio ───────────────────────────
const P5 = BASE + "/nao-existe-ainda.pickle";
if (fs.exists(P5)) { fs.remove_file(P5); }
const novo = new Storage();
novo.persistTo(P5);
const novoLen = novo.length;

describe("Storage (Web Storage API) em Rust", () => {
  test("API em memória segue a spec", () => {
    expect(memVazio).toBe(0);
    expect(memLen).toBe(2);
    expect(memA).toBe("1");
    expect(memAusente).toBe(null);
    expect(memKey0).toBe("a");
    expect(memKeyFora).toBe(null);
  });

  test("setItem sobrescreve sem duplicar a chave", () => {
    expect(memSobrescrito).toBe("9");
    expect(memLenAposSobrescrever).toBe(2);
  });

  test("removeItem e clear", () => {
    expect(aposRemove).toBe(1);
    expect(xRemovido).toBe(null);
    expect(aposRemoveAusente).toBe(1);
    expect(aposClear).toBe(0);
  });
});

describe("Storage — persistência via pickle", () => {
  test("sobrevive entre execuções", () => {
    expect(arquivoCriado).toBe(1);
    expect(leUser).toBe("ana");
    expect(leVisitas).toBe("3");
    expect(leLen).toBe(2);
    expect(leOrdem).toBe("user");
  });

  test("valor com \\n, \\t e separadores volta byte a byte", () => {
    expect(voltouHostil).toBe(hostil);
  });

  test("removeItem e clear persistem", () => {
    expect(persistidoAposRemove).toBe(1);
    expect(aChaveSumiu).toBe(null);
    expect(persistidoAposClear).toBe(0);
  });

  test("arquivo corrompido não derruba e o storage segue utilizável", () => {
    expect(corrompidoLen).toBe(0);
    expect(corrompidoUsavel).toBe("ok");
  });

  test("arquivo inexistente é um storage vazio", () => {
    expect(novoLen).toBe(0);
  });
});
