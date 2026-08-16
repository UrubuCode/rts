// Cross-runtime: labeled break exits a nested non-loop block.
const seen: string[] = [];
section: {
  seen.push("start");
  {
    seen.push("inner");
    break section;
  }
  seen.push("miss");
}
seen.push("end");
console.log(seen.join(","));

