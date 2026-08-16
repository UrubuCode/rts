// Cross-runtime: Promise chaining chooses its constructor through Symbol.species.
class BaseResult<T> extends Promise<T> {}
class Source<T> extends Promise<T> {
  static get [Symbol.species]() { return BaseResult; }
}
const source = new Source<number>((resolve) => resolve(3));
const thenResult = source.then((x) => x * 2);
const finallyResult = source.finally(() => {});
console.log(thenResult instanceof Source, thenResult instanceof BaseResult);
console.log(finallyResult instanceof Source, finallyResult instanceof BaseResult);
Promise.all([thenResult, finallyResult]).then((v) => console.log(v.join(",")));

