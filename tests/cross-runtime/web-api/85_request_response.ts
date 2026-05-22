// Cross-runtime compatibility: Request/Response primitives.
const res = new Response(JSON.stringify({ a: 1 }), {
  headers: { "content-type": "application/json" },
});
console.log("ct=" + res.headers.get("content-type"));
console.log("txt=" + (await res.text()));

const req = new Request("https://example.com/x", { method: "POST", body: "hi" });
console.log("m=" + req.method);
console.log("u=" + req.url);
console.log("b=" + (await req.text()));
