// Cross-runtime: Promise.finally waits for thenable and preserves value.
const log: string[] = [];
Promise.resolve("value")
  .finally(() => {
    log.push("finally");
    return { then(resolve: () => void) { log.push("thenable"); resolve(); } };
  })
  .then(v => {
    log.push(v);
    console.log(log.join("|"));
  });
