// Cross-runtime: Promise resolution reads a then getter exactly once.
let reads = 0;
const value = {
  get then() {
    reads++;
    return (resolve: any) => resolve("ok");
  },
};
Promise.resolve(value).then((result) => {
  console.log(result, reads);
  console.log(reads === 1);
});

