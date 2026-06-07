// Cross-runtime: Promise thenable assimilation and single-resolution rule.
const events: string[] = [];

const thenable = {
  then(resolve: (v: string) => void, reject: (e: string) => void) {
    events.push("then");
    resolve("ok");
    reject("bad");
    resolve("again");
  }
};

Promise.resolve(thenable)
  .then(v => {
    events.push("value:" + v);
    return Promise.resolve("next");
  })
  .then(v => {
    events.push(v);
    console.log(events.join("|"));
  });
