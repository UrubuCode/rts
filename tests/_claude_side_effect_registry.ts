// The shared state the side-effect modules beside this one write into.
//
// A registry rather than `console.log`, because what the test pins is that a
// module RAN — and a counter can say "exactly once" where printed output only
// says "at least once".
export const order: string[] = [];
export const store: string[] = [];
export const runs = { n: 0 };
export function add(name: string): void {
  store.push(name);
}
