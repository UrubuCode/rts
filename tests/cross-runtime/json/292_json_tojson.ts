// Cross-runtime: JSON.stringify honoring toJSON.
class C { toJSON() { return { x: 7 }; } }
console.log("cls=" + JSON.stringify(new C()));
console.log("date=" + JSON.stringify(new Date(Date.UTC(2024, 4, 10, 12, 34, 56, 789))));
