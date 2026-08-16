// Cross-runtime: matchAll yields captures and source indexes for every global match.
const rows = [..."x1 yy22 z333".matchAll(/([a-z]+)(\d+)/g)];
console.log(rows.map((m) => [m[0], m[1], m[2], m.index].join(":")).join("|"));
console.log(rows.length);

