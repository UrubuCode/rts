// Um módulo SÓ de tipos: nada disto sobrevive ao type-strip.
//
// Existe para `claude-module-type-only.test.ts`, e o ponto é precisamente que
// não exporta nada em tempo de execução.
export interface Shape {
  side: number;
}
export type Name = string;
