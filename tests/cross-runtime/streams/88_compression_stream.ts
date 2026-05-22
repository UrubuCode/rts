// Cross-runtime compatibility: CompressionStream gzip output length.
const cs = new CompressionStream("gzip");
const writer = cs.writable.getWriter();
await writer.write(new TextEncoder().encode("hello hello"));
await writer.close();

const reader = cs.readable.getReader();
let total = 0;
while (true) {
  const chunk = await reader.read();
  if (chunk.done) break;
  total += chunk.value.length;
}
console.log("gzip=" + total);
