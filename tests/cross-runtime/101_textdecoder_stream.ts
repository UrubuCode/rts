// Cross-runtime compatibility: TextEncoderStream/TextDecoderStream.
const encoder = new TextEncoderStream();
const decoder = new TextDecoderStream();
const writer = encoder.writable.getWriter();
const reader = encoder.readable.pipeThrough(decoder).getReader();

const writing = (async () => {
  await writer.write("caf");
  await writer.write("é");
  await writer.close();
})();

const out: string[] = [];
while (true) {
  const part = await reader.read();
  if (part.done) break;
  out.push(String(part.value));
}
await writing;
console.log("out=" + out.join("|"));
