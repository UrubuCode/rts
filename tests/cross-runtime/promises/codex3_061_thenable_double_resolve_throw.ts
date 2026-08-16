// Cross-runtime: thenable assimilation accepts only the first settlement and ignores later throw.
const seen: string[] = [];
const thenable = {
  then(resolve: any, reject: any) {
    seen.push("then");
    resolve("first");
    reject("second");
    resolve("third");
    throw new Error("late");
  },
};
Promise.resolve(thenable).then(
  (value) => seen.push("fulfilled:" + value),
  (reason) => seen.push("rejected:" + reason),
).then(() => console.log(seen.join("|")));

