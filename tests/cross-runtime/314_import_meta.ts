// Cross-runtime: import.meta basics.
console.log("url=" + String(import.meta.url.startsWith("file:")));
console.log("main=" + String(typeof import.meta === "object"));
