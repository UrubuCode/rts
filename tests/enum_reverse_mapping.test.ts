import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): enum numerico nao tinha reverse mapping —
// `State[0]` dava 0 em vez de "Idle". JS gera entradas reversas `[value]:
// "name"` para enums numericos. Fix: o desugar enum->const adiciona a prop
// reversa quando o valor eh inteiro literal. String enums nao tem reverse.

let out = "";
function print(v: string): void { out += v + "\n"; }

enum State { Idle, Running, Done }

// forward continua OK
print(State.Running + "");        // 1
print(State.Done + "");          // 2

// reverse mapping
print(State[0]);                 // Idle
print(State[1]);                 // Running
print(State[2]);                 // Done

// reverse via var
const k = 2;
print(State[k]);                 // Done

// enum com valores explicitos
enum Code { Ok = 200, NotFound = 404 }
print(Code.Ok + "");             // 200
print(Code[404]);                // NotFound

// string enum NAO tem reverse (forward so')
enum Color { Red = "RED", Blue = "BLUE" }
print(Color.Red);                // RED

describe("enum reverse mapping", () => {
  test("enum numerico tem reverse mapping bidirecional", () =>
    expect(out).toBe("1\n2\nIdle\nRunning\nDone\nDone\n200\nNotFound\nRED\n"));
});
