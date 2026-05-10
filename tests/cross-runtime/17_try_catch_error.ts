// Cross-runtime: try/catch/finally, Error subclasses and rethrow.
try {
  throw new Error("boom");
} catch (e: any) {
  console.log("err_a=" + e.name + ":" + e.message);
} finally {
  console.log("fin_a=done");
}

try {
  throw "plain";
} catch (e: any) {
  console.log("err_b=" + e);
}

try {
  try {
    throw new TypeError("bad");
  } catch (e: any) {
    console.log("inner=" + e.name);
    throw new RangeError(e.message + ":range");
  }
} catch (e: any) {
  console.log("outer=" + e.name + ":" + e.message);
  console.log("is_error=" + (e instanceof Error));
  console.log("is_type=" + (e instanceof TypeError));
  console.log("is_range=" + (e instanceof RangeError));
}
