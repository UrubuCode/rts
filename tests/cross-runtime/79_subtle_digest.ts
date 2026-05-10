// Cross-runtime compatibility: WebCrypto subtle digest.
const data = new TextEncoder().encode("abc");
const digest = await crypto.subtle.digest("SHA-256", data);
const hex = Array.from(new Uint8Array(digest))
  .slice(0, 8)
  .map((x) => x.toString(16).padStart(2, "0"))
  .join("");
console.log("sha256_8=" + hex);
