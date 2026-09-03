// One leaf of the graph: no imports of its own, so it is the first module to
// run and the one every other file below reads through.
export const LABEL = "graph";

export function twice(n: number): number {
  return n * 2;
}

export function* upto(n: number) {
  for (let i = 1; i <= n; i++) yield i;
}
