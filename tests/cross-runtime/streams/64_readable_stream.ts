// Cross-runtime compatibility: ReadableStream basic pull.
const stream = new ReadableStream({
  start(controller) {
    controller.enqueue("a");
    controller.enqueue("b");
    controller.close();
  },
});

async function main() {
  const reader = stream.getReader();
  const out: string[] = [];
  while (true) {
    const part = await reader.read();
    if (part.done) break;
    out.push(String(part.value));
  }
  console.log("out=" + out.join(","));
}

main();
