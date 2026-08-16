// Cross-runtime: a stack machine interpreting a bytecode array — dispatch over
// a switch, a call stack with frames, locals, jumps and a trace of every step.
// It stresses the engine (arrays, closures, string building) rather than an API.

const OP_PUSH = 0;
const OP_LOAD = 1;
const OP_STORE = 2;
const OP_ADD = 3;
const OP_SUB = 4;
const OP_MUL = 5;
const OP_LT = 6;
const OP_JMP = 7;
const OP_JZ = 8;
const OP_CALL = 9;
const OP_RET = 10;
const OP_PRINT = 11;
const OP_HALT = 12;
const OP_DUP = 13;
const OP_POP = 14;

const NAMES = ["PUSH", "LOAD", "STORE", "ADD", "SUB", "MUL", "LT", "JMP", "JZ", "CALL", "RET", "PRINT", "HALT", "DUP", "POP"];

interface Frame {
  ret: number;
  locals: number[];
  base: number;
}

function run(code: number[], entry: number, traceLimit: number): string[] {
  const out: string[] = [];
  const stack: number[] = [];
  const frames: Frame[] = [{ ret: -1, locals: [0, 0, 0, 0], base: 0 }];
  let pc = entry;
  let steps = 0;

  while (true) {
    if (steps >= 5000) { out.push("ABORT"); break; }
    const op = code[pc];
    const frame = frames[frames.length - 1];
    if (steps < traceLimit) {
      out.push(steps + " pc=" + pc + " " + NAMES[op] + " stack=[" + stack.join(",") + "]");
    }
    pc += 1;
    steps += 1;

    switch (op) {
      case OP_PUSH: stack.push(code[pc]); pc += 1; break;
      case OP_LOAD: stack.push(frame.locals[code[pc]]); pc += 1; break;
      case OP_STORE: frame.locals[code[pc]] = stack.pop() as number; pc += 1; break;
      case OP_ADD: { const b = stack.pop() as number; stack.push((stack.pop() as number) + b); break; }
      case OP_SUB: { const b = stack.pop() as number; stack.push((stack.pop() as number) - b); break; }
      case OP_MUL: { const b = stack.pop() as number; stack.push((stack.pop() as number) * b); break; }
      case OP_LT: { const b = stack.pop() as number; stack.push((stack.pop() as number) < b ? 1 : 0); break; }
      case OP_JMP: pc = code[pc]; break;
      case OP_JZ: { const t = code[pc]; pc += 1; if ((stack.pop() as number) === 0) pc = t; break; }
      case OP_CALL: {
        const target = code[pc];
        const argc = code[pc + 1];
        pc += 2;
        const locals = [0, 0, 0, 0];
        for (let i = argc - 1; i >= 0; i--) locals[i] = stack.pop() as number;
        frames.push({ ret: pc, locals: locals, base: stack.length });
        pc = target;
        break;
      }
      case OP_RET: {
        const value = stack.pop() as number;
        const done = frames.pop() as Frame;
        stack.length = done.base;
        stack.push(value);
        pc = done.ret;
        break;
      }
      case OP_PRINT: out.push("  out " + stack[stack.length - 1]); break;
      case OP_DUP: stack.push(stack[stack.length - 1]); break;
      case OP_POP: stack.pop(); break;
      case OP_HALT: out.push("HALT top=" + (stack.length === 0 ? "empty" : String(stack[stack.length - 1])) + " steps=" + steps); return out;
      default: out.push("BAD op " + op); return out;
    }
  }
  return out;
}

// Program 1: (3 + 4) * 5, printed then halted.
const arith = [
  OP_PUSH, 3,
  OP_PUSH, 4,
  OP_ADD,
  OP_PUSH, 5,
  OP_MUL,
  OP_PRINT,
  OP_HALT,
];
console.log("--- arithmetic");
for (const line of run(arith, 0, 20)) console.log(line);

// Program 2: a counted loop summing 1..5 in local 1 with local 0 as the counter.
const loop = [
  /* 0 */ OP_PUSH, 0, OP_STORE, 1,
  /* 4 */ OP_PUSH, 1, OP_STORE, 0,
  /* 8 */ OP_LOAD, 0, OP_PUSH, 6, OP_LT,
  /* 13 */ OP_JZ, 29,
  /* 15 */ OP_LOAD, 1, OP_LOAD, 0, OP_ADD, OP_STORE, 1,
  /* 22 */ OP_LOAD, 0, OP_PUSH, 1, OP_ADD, OP_STORE, 0,
  /* 29 is the exit */
];
loop[13] = OP_JZ; loop[14] = 30;
loop.push(OP_JMP, 8);
loop.push(OP_LOAD, 1, OP_PRINT, OP_HALT);
console.log("--- loop sum 1..5");
for (const line of run(loop, 0, 12)) console.log(line);

// Program 3: recursive factorial through CALL/RET.
// fact(n) = n < 2 ? 1 : n * fact(n - 1)
const FACT = 12;
const factorial = [
  /* 0 */ OP_PUSH, 5,
  /* 2 */ OP_CALL, FACT, 1,
  /* 5 */ OP_PRINT,
  /* 6 */ OP_HALT,
  /* 7 */ OP_HALT, OP_HALT, OP_HALT, OP_HALT, OP_HALT,
  /* 12 */ OP_LOAD, 0, OP_PUSH, 2, OP_LT,
  /* 17 */ OP_JZ, 22,
  /* 19 */ OP_PUSH, 1, OP_RET,
  /* 22 */ OP_LOAD, 0,
  /* 24 */ OP_LOAD, 0, OP_PUSH, 1, OP_SUB,
  /* 29 */ OP_CALL, FACT, 1,
  /* 32 */ OP_MUL, OP_RET,
];
console.log("--- factorial 5");
for (const line of run(factorial, 0, 14)) console.log(line);

// Program 4: mutual work through two frames — double(triple(2)).
const DOUBLE = 10;
const TRIPLE = 16;
const nested = [
  /* 0 */ OP_PUSH, 2,
  /* 2 */ OP_CALL, TRIPLE, 1,
  /* 5 */ OP_CALL, DOUBLE, 1,
  /* 8 */ OP_PRINT, OP_HALT,
  /* 10 */ OP_LOAD, 0, OP_PUSH, 2, OP_MUL, OP_RET,
  /* 16 */ OP_LOAD, 0, OP_PUSH, 3, OP_MUL, OP_RET,
];
console.log("--- double(triple(2))");
for (const line of run(nested, 0, 30)) console.log(line);

// Program 5: an unknown opcode is reported rather than crashing.
console.log("--- bad opcode");
for (const line of run([OP_PUSH, 1, 99, OP_HALT], 0, 5)) console.log(line);

// Program 6: DUP/POP and a conditional that falls through.
const stackOps = [
  OP_PUSH, 7, OP_DUP, OP_ADD, OP_DUP, OP_PRINT, OP_PUSH, 0, OP_JZ, 12,
  OP_POP, OP_HALT,
  /* 12 */ OP_PUSH, 100, OP_ADD, OP_PRINT, OP_HALT,
];
console.log("--- stack ops");
for (const line of run(stackOps, 0, 20)) console.log(line);
