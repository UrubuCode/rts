// Cross-runtime: finally executes before loop continue transfers control.
const seen: string[] = [];
for (let i = 0; i < 3; i++) {
  try {
    seen.push("t" + i);
    if (i < 2) continue;
  } finally {
    seen.push("f" + i);
  }
  seen.push("a" + i);
}
console.log(seen.join(","));

