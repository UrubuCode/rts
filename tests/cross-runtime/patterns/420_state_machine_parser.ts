// Cross-runtime: small hand-written parser/state machine.
function parseExpr(src: string): number {
  let i = 0;
  function skip() { while (src[i] === " ") i++; }
  function num(): number {
    skip();
    let s = "";
    while (/[0-9]/.test(src[i] || "")) s += src[i++];
    return Number(s);
  }
  function factor(): number {
    skip();
    if (src[i] === "(") {
      i++;
      const v = expr();
      skip();
      i++;
      return v;
    }
    return num();
  }
  function term(): number {
    let v = factor();
    for (;;) {
      skip();
      if (src[i] === "*") { i++; v *= factor(); }
      else break;
    }
    return v;
  }
  function expr(): number {
    let v = term();
    for (;;) {
      skip();
      if (src[i] === "+") { i++; v += term(); }
      else if (src[i] === "-") { i++; v -= term(); }
      else break;
    }
    return v;
  }
  return expr();
}

console.log(parseExpr("2+3*4"));
console.log(parseExpr("(2+3)*4-5"));
console.log(parseExpr("10 + 6 * (7 - 4)"));
