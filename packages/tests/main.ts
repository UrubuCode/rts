import { io } from "rts";
import { testArithmeticExpressions } from "./arithmetic";
import { testComplexFunctions } from "./functions";
import { testDeclarations } from "./declarations";

export function runAllRtsTests(): void {
  io.print("[tests] start");
  testArithmeticExpressions();
  testComplexFunctions();
  testDeclarations();
  io.print("[tests] done");
}
