// Cross-runtime compatibility: error stack presence without pinning full format.
function boom() {
  throw new Error("zap");
}

try {
  boom();
} catch (e: any) {
  console.log("name=" + e.name);
  console.log("msg=" + e.message);
  console.log("stack=" + (typeof e.stack));
  console.log("hasBoom=" + String(String(e.stack).includes("boom")));
}
