// Cross-runtime compatibility: Blob basic behavior.
const blob = new Blob(["hi", new Uint8Array([33])]);
console.log("size=" + blob.size);
console.log("text=" + (await blob.text()));
