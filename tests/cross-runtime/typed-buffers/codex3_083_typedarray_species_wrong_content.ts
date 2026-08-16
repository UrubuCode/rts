// Cross-runtime: typed-array species may not switch between Number and BigInt content types.
class Bad extends Uint8Array {
  static get [Symbol.species]() { return BigInt64Array; }
}
const value = new Bad([1, 2, 3]);
const checks: boolean[] = [];
try { value.slice(); } catch (e) { checks.push(e instanceof TypeError); }
try { value.map((x) => x); } catch (e) { checks.push(e instanceof TypeError); }
console.log(checks.join(","));

