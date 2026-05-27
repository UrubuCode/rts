import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Construcao via array literal
const u8 = new Uint8Array([1, 2, 3, 4]);
print("len=" + u8.length);
print("u8[0]=" + u8[0]);
print("u8[3]=" + u8[3]);
u8[1] = 10;
print("u8[1]=" + u8[1]);

// Construcao via tamanho (zeros)
const u16 = new Uint16Array(3);
u16[0] = 100;
u16[1] = 200;
u16[2] = 300;
print("u16=" + Array.from(u16).join(","));

// Int32 com negativos
const i32 = new Int32Array([1, -2, 3]);
print("i32=" + Array.from(i32).join(","));

// Int8 sign-extend (200 -> -56 em Int8) e Uint8 wrap (300 -> 44)
const i8 = new Int8Array([200]);
print("i8wrap=" + i8[0]);
const u8b = new Uint8Array([300]);
print("u8wrap=" + u8b[0]);

// Soma dentro de loop
const data = new Int32Array([10, 20, 30, 40]);
let sum = 0;
for (let i = 0; i < data.length; i++) sum += data[i];
print("sum=" + sum);

describe("typedarray_basic (#810)", () => {
  test("output completo", () =>
    expect(out).toBe(
      "len=4\n" +
      "u8[0]=1\n" +
      "u8[3]=4\n" +
      "u8[1]=10\n" +
      "u16=100,200,300\n" +
      "i32=1,-2,3\n" +
      "i8wrap=-56\n" +
      "u8wrap=44\n" +
      "sum=100\n"
    ));
});
