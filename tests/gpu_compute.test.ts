import { describe, test, expect } from "rts:test";
import gpu from "rts:gpu";
import buffer from "rts:buffer";

// rts:gpu — compute WGSL no device compartilhado.
//
// Numa máquina SEM GPU (ou driver quebrado) `available()` é 0 e TODO o resto
// devolve 0/-1 sem crash — os testes então só verificam essa degradação limpa.
// Valores pré-computados no top-level (chamar métodos dentro do closure de
// test() pode pegar GC de handle coletado — regra da suíte).

const av = gpu.available();

// Kernel: d[i] = d[i] * 2 + 1 (validável elemento a elemento).
const N = 256;
const pipe = av === 1 ? gpu.shader(`
@group(0) @binding(0) var<storage, read_write> d: array<f32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  d[id.x] = d[id.x] * 2.0 + 1.0;
}
`) : 0;

const cpu = buffer.alloc(N * 4);
let __i = 0;
while (__i < N) { buffer.write_f32(cpu, __i * 4, __i * 1.0); __i = __i + 1; }

let wrote = 0;
let bound = 0;
let dispatched = 0;
let readBytes = 0;
let okAll = 0;
let badShader = 0;
let freed = 0;
let dispatchAfterFree = 0;

if (av === 1) {
  const gbuf = gpu.buffer(N * 4);
  wrote = gpu.write(gbuf, cpu, N * 4);
  bound = gpu.bind_buffer(pipe, 0, gbuf);
  dispatched = gpu.dispatch(pipe, N / 64, 1, 1);
  readBytes = gpu.read(gbuf, cpu, N * 4);
  okAll = 1;
  let i = 0;
  while (i < N) {
    if (buffer.read_f32(cpu, i * 4) !== i * 2.0 + 1.0) { okAll = 0; }
    i = i + 1;
  }
  // WGSL inválido: 0, sem derrubar o runtime.
  badShader = gpu.shader("isto nao é wgsl");
  // buffer liberado some dos binds; dispatch seguinte falha a validação (0).
  freed = gpu.buffer_free(gbuf);
  dispatchAfterFree = gpu.dispatch(pipe, 1, 1, 1);
}

describe("rts:gpu compute", () => {
  if (av === 0) {
    test("sem GPU: available()=0 e degradação limpa", () => {
      expect(gpu.shader("x")).toBe(0);
      expect(gpu.buffer(64)).toBe(0);
      expect(gpu.read(1, cpu, 64)).toBe(0 - 1);
    });
    return;
  }
  test("shader compila e devolve handle", () => {
    expect(pipe > 0).toBeTruthy();
  });
  test("write/bind/dispatch/read reportam sucesso", () => {
    expect(wrote).toBe(1);
    expect(bound).toBe(1);
    expect(dispatched).toBe(1);
    expect(readBytes).toBe(N * 4);
  });
  test("kernel d[i]=d[i]*2+1 confere nos 256 elementos", () => {
    expect(okAll).toBe(1);
  });
  test("WGSL inválido devolve 0 (sem crash)", () => {
    expect(badShader).toBe(0);
  });
  test("buffer_free desliga o binding e o dispatch seguinte falha limpo", () => {
    expect(freed).toBe(1);
    expect(dispatchAfterFree).toBe(0);
  });
});
