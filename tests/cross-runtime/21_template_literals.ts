// Cross-runtime: template literals, escapes, multiline and call expressions.
function tag(x: string): string {
  return x.toUpperCase();
}

const who = "rts";
const count = 2 + 3;

console.log(`basic=${who}:${count}`);
console.log(`nested=${`${who}-ok`}:${tag("go")}`);
console.log(`escapes=line1\nline2\tend`);
console.log(`multi=a
b`);
console.log(`call=${tag(who)}:${"x".repeat(2)}`);
