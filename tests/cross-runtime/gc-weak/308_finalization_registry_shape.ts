// Cross-runtime: FinalizationRegistry API shape.
const fr = new FinalizationRegistry(() => {});
console.log("type=" + typeof FinalizationRegistry);
console.log("reg=" + typeof fr.register);
console.log("unreg=" + typeof fr.unregister);
