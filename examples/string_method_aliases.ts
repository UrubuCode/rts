import { io } from "rts";

// Mesma API JS nativa de String como metodo — o lowering reescreve
// para `str.*` no namespace automaticamente.
function main(): void {
  const s = "foo bar foo baz foo";

  // replaceAll (nao nativo em JS antigo mas padrao ES2021)
  io.print(s.replaceAll("foo", "X"));        // X bar X baz X

  // replace (so o primeiro)
  io.print(s.replace("foo", "X"));           // X bar foo baz foo

  // indexOf / lastIndexOf
  io.print(s.indexOf("foo"));                // 0
  io.print(s.lastIndexOf("foo"));            // 16

  // startsWith / endsWith / includes
  io.print(s.startsWith("foo"));             // true
  io.print(s.endsWith("foo"));               // true
  io.print(s.includes("bar"));               // true

  // toUpperCase / toLowerCase
  io.print("Mixed".toUpperCase());           // MIXED
  io.print("Mixed".toLowerCase());           // mixed

  // slice / trim
  io.print("  hello  ".trim());              // hello
  io.print("abcdef".slice(1, 4));            // bcd
}
