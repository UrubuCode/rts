// Cross-runtime: then getter is read once during thenable assimilation.
let count = 0;
const obj = {
  get then() {
    count++;
    return (resolve: (v: string) => void) => resolve("ok");
  }
};

Promise.resolve(obj).then(v => {
  console.log(v);
  console.log(count);
});
