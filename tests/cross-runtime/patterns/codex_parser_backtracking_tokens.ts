// Cross-runtime: tiny backtracking parser with saved cursor state.
function parse(input: string): string {
  let i = 0;
  function eat(s: string) {
    if (input.slice(i, i + s.length) === s) { i += s.length; return true; }
    return false;
  }
  function alt(): string {
    const save = i;
    if (eat("ab") && eat("c")) return "abc";
    i = save;
    if (eat("ab")) return "ab";
    i = save;
    return eat("a") ? "a" : "none";
  }
  const first = alt();
  const pos = i;
  const second = alt();
  return first + "@" + pos + "|" + second + "@" + i;
}

console.log(parse("abcab"));
console.log(parse("ababca"));
