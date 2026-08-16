// Cross-runtime: new.target distinguishes calls, construction, and bound construction.
function Probe(this: any) {
  return new.target ? new.target.name : "call";
}
const Bound: any = Probe.bind({ ignored: true });
console.log(Probe());
console.log(new (Probe as any)().constructor === Probe);
console.log(new Bound().constructor === Probe);

