// Cross-runtime: Promise.all preserves input order despite resolution order.
const log: string[] = [];
const slow = new Promise<string>(resolve => {
  log.push("slow-start");
  setTimeout(() => { log.push("slow-end"); resolve("slow"); }, 0);
});
const thenable = {
  then(resolve: (v: string) => void) {
    log.push("thenable");
    resolve("then");
  }
};

Promise.all([slow, Promise.resolve("fast"), thenable]).then(values => {
  console.log(values.join(","));
  console.log(log.join("|"));
});
