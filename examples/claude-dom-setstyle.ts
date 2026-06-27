// Testa o estilo IMPERATIVO por-nó: setStyle/override por-nó (setStyleBatch,
// invariante 6 do F2) que VENCE o estilo de tag e o style="" inline. Headless.
// Usa os PRIMITIVOS rts:dom diretos (dom.setStyle / dom.nodeStyleSlot) — o método
// de classe el.setStyle() sobre array[i] baila no motor (receiver não despachável;
// limite conhecido). Slots: 0=color 2=font_size.
//   target/release/rts.exe run examples/claude-dom-setstyle.ts

import dom from "rts:dom";

const SLOT_COLOR = 0;
const SLOT_FONT_SIZE = 2;

const d = dom.parseHtml(
  "<ul><li>A</li><li style='color:#0000ff'>B</li><li>C</li></ul>"
);

// itera os <li> via count+at de NodeIds crus (o padrão estável do motor: NodeId é
// number, sem classe envolvida no laço).
const n = dom.querySelectorAllCount(d, "li");
console.log("li count: " + n); // 3

let i = 0;
while (i < n) {
  const li = dom.querySelectorAllAt(d, "li", i); // NodeId (number)
  // estilo por-NÓ imperativo: verde + tamanho crescente. No #B (inline azul) o
  // override DEVE vencer.
  dom.setStyle(d, li, SLOT_COLOR, 0x00AA00FF);
  dom.setStyle(d, li, SLOT_FONT_SIZE, 16 + i * 4);
  i = i + 1;
}

// lê de volta o computado (inclui o override) para provar que pegou.
let j = 0;
while (j < n) {
  const li = dom.querySelectorAllAt(d, "li", j);
  const color = dom.nodeStyleSlot(d, li, SLOT_COLOR);
  const size = dom.nodeStyleSlot(d, li, SLOT_FONT_SIZE);
  console.log("li[" + j + "] color=" + color + " size=" + size);
  j = j + 1;
}

// setStyleBatch: aplica várias triplas de uma vez via buffer (a forma do
// invariante 6). Escreve (nodeId, slot, val) i64 LE no buffer.
import buffer from "rts:buffer";
const liA = dom.querySelectorAllAt(d, "li", 0);
const liC = dom.querySelectorAllAt(d, "li", 2);
const buf = buffer.alloc(2 * 3 * 8); // 2 triplas × 3 i64 × 8 bytes
buffer.write_i64(buf, 0, liA);
buffer.write_i64(buf, 8, SLOT_COLOR);
buffer.write_i64(buf, 16, 0xFF0000FF); // A vira vermelho
buffer.write_i64(buf, 24, liC);
buffer.write_i64(buf, 32, SLOT_COLOR);
buffer.write_i64(buf, 40, 0xFFFF00FF); // C vira amarelo
dom.setStyleBatch(d, buf, 2);
console.log("apos batch, liA color=" + dom.nodeStyleSlot(d, liA, SLOT_COLOR)); // -255 (0xFF0000FF como i64)
console.log("apos batch, liC color=" + dom.nodeStyleSlot(d, liC, SLOT_COLOR));

console.log("=== fim ===");
