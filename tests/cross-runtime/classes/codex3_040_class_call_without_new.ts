// Cross-runtime: class constructors throw when invoked without new through call or Reflect.apply.
class OnlyNew { value = 1; }
const results: boolean[] = [];
try { (OnlyNew as any)(); } catch (e) { results.push(e instanceof TypeError); }
try { OnlyNew.call({}); } catch (e) { results.push(e instanceof TypeError); }
try { Reflect.apply(OnlyNew as any, {}, []); } catch (e) { results.push(e instanceof TypeError); }
console.log(results.join(","));
console.log(new OnlyNew().value);

