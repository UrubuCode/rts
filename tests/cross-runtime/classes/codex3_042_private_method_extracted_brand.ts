// Cross-runtime: an extracted method still enforces private-field receiver branding.
class Secret {
  #value = 8;
  read() { return this.#value; }
}
const value = new Secret();
const read = value.read;
console.log(read.call(value));
const checks: boolean[] = [];
for (const receiver of [{}, Object.create(Secret.prototype)]) {
  try { read.call(receiver); checks.push(false); } catch (e) { checks.push(e instanceof TypeError); }
}
console.log(checks.join(","));

