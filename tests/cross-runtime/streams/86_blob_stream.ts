// Cross-runtime compatibility: Blob.stream().
const blob = new Blob(["ab"]);
const reader = blob.stream().getReader();
const parts = [];
while (true) {
  const chunk = await reader.read();
  if (chunk.done) break;
  parts.push(...chunk.value);
}
console.log("bytes=" + parts.join(","));
