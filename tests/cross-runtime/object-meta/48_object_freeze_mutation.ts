// Cross-runtime compatibility: Object.freeze observable state.
const frozen = Object.freeze({ a: 1, b: "x" });
console.log("frozen=" + Object.isFrozen(frozen));
console.log("keys=" + Object.keys(frozen).join(","));
console.log("json=" + JSON.stringify(frozen));
