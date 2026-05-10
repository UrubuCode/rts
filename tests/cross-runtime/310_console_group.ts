// Cross-runtime: console.group/groupEnd API shape.
const events: string[] = [];
const g = console.group;
const ge = console.groupEnd;
(console as any).group = (...args: any[]) => { events.push("g:" + args.join(",")); };
(console as any).groupEnd = () => { events.push("end"); };
console.group("x");
console.groupEnd();
(console as any).group = g;
(console as any).groupEnd = ge;
console.log("events=" + events.join("|"));
