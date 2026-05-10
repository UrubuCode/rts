// Cross-runtime: Promise.finally
const log: string[] = [];

Promise.resolve(42)
  .then(val => {
    log.push("then:" + val);
    return val * 2;
  })
  .finally(() => {
    log.push("finally1");
  })
  .then(val => {
    log.push("then2:" + val);
  });

Promise.reject("error")
  .catch(err => {
    log.push("catch:" + err);
  })
  .finally(() => {
    log.push("finally2");
  });

setTimeout(() => {
  console.log("log=" + log.join(","));
}, 10);
