// Error hierarchy, rethrow, nested try, custom errors, cause.
const out = [];
class AppError extends Error {
  constructor(msg, code) { super(msg); this.name = "AppError"; this.code = code; }
}
try { throw new AppError("bad", 42); }
catch (e) { out.push(e.name, e.message, e.code, e instanceof Error, e instanceof AppError); }
function nested(n) {
  try {
    try { if (n) throw new TypeError("inner"); return "no"; }
    catch (e) { throw new RangeError("wrapped:" + e.message); }
    finally { out.push("f1"); }
  } catch (e) { return e.name + ":" + e.message; }
  finally { out.push("f2"); }
}
out.push(nested(1), nested(0));
const withCause = new Error("outer", { cause: new Error("root") });
out.push(withCause.cause.message);
try { null.x; } catch (e) { out.push(e.constructor.name); }
try { (undefined)(); } catch (e) { out.push(e.constructor.name); }
try { JSON.parse("{bad"); } catch (e) { out.push(e.constructor.name); }
out.push([1, 2].map((n) => { try { if (n === 1) throw new Error("s"); return "ok"; } catch { return "caught"; } }).join(","));
console.log(out.join("|"));
