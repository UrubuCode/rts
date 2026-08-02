import { describe, test, expect } from "rts:test";

// Fecha o par da CADEIA DE ESCOPO, junto com #2078 (leitura) e #2098 (escrita):
//
// 1. `x++` / `--x` sobre um nome LIVRE compõe os dois trampolins — lê pelo
//    `global_ref`, escreve pelo `global_set` —, exatamente como o caminho de
//    célula já fazia. Antes recusava o ARQUIVO INTEIRO.
//
// 2. Uma CLOSURE que CAPTURA um nome livre materializa a captura pelo mesmo
//    `global_ref`. O `build_closure_env` só sabia ler local, função de topo,
//    gcell e classe (#2095); um GLOBAL — inclusive um global implícito — não
//    tinha morada e o arquivo caía, embora o nome EXISTA.
//
// NÃO É FIXTURE CROSS-RUNTIME pelo mesmo motivo do #2098: Node e Bun discordam
// sobre o MODO de um `.ts` (script sloppy vs módulo strict), então uma fixture
// documentaria a escolha de módulo do runtime, não o motor. Valores conferidos
// contra o Node.

declare var cnt: any;
declare var dec: any;
declare var visto: any;

// ── `++`/`--` sobre nome livre ──────────────────────────────────────────────
cnt = 0;
function posfixo(): any { return cnt++; }
const p1 = posfixo();
const p2 = posfixo();
const depoisPos = cnt;

function prefixo(): any { return ++cnt; }
const pre = prefixo();

dec = 5;
const decPos = dec--;
const depoisDec = dec;

// ── closure capturando nome livre ───────────────────────────────────────────
visto = "G";
const leCaptura = () => visto + "!";
const cap1 = leCaptura();
visto = "H";
const cap2 = leCaptura();
const viaMap = [1].map(() => visto)[0];

describe("nome livre: ++/-- e captura por closure", () => {
  test("pós-fixo devolve o antigo e escreve o global", () => {
    expect(p1).toBe(0);
    expect(p2).toBe(1);
    expect(depoisPos).toBe(2);
  });
  test("pré-fixo devolve o novo", () => {
    expect(pre).toBe(3);
  });
  test("decremento pós-fixo", () => {
    expect(decPos).toBe(5);
    expect(depoisDec).toBe(4);
  });
  test("closure lê o valor CORRENTE do global, não um instantâneo", () => {
    expect(cap1).toBe("G!");
    expect(cap2).toBe("H!");
  });
  test("captura de nome livre dentro de callback", () => {
    expect(viaMap).toBe("H");
  });
});
