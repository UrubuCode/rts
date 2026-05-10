// Cross-runtime compatibility: TransformStream pipeline.
const stream = new TransformStream({
  transform(chunk, controller) {
    controller.enqueue(String(chunk).toUpperCase());
  },
});

const writer = stream.writable.getWriter();
const reader = stream.readable.getReader();

const writing = (async () => {
  await writer.write("ab");
  await writer.write("cd");
  await writer.close();
})();

const out: string[] = [];
while (true) {
  const part = await reader.read();
  if (part.done) break;
  out.push(String(part.value));
}
await writing;
console.log("out=" + out.join(","));
