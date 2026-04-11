import { io } from "rts";

// Demonstra que métodos estáticos de classe agora compilam
// até o MIR e viram símbolos no objeto nativo (pedaço 0a).
//
// Limitação conhecida: a chamada `Calc.add(2, 3)` ainda depende
// de `Expr::Member` completo, que é trabalho do pedaço seguinte
// (feat/missing-exprs). Por enquanto, o método existe como símbolo
// mas não é invocável via nome qualificado no código TS.
class Calc {
  static add(a: number, b: number): number {
    return a + b;
  }
}

function main(): void {
  io.print("method bodies sans this: classe compilou");
}
