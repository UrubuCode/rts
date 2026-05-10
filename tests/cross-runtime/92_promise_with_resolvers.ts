// Cross-runtime compatibility: Promise.withResolvers.
const kit = Promise.withResolvers<string>();
kit.resolve("ok");
console.log("value=" + (await kit.promise));
