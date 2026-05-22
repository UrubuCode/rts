// Cross-runtime: optional chaining ?. operator.
type User = { name: string; address?: { city?: string } };

const u1: User = { name: "Alice", address: { city: "NYC" } };
const u2: User = { name: "Bob", address: {} };
const u3: User = { name: "Carol" };
const u4: User | null = null;

console.log(u1.address?.city);
console.log(u2.address?.city);
console.log(u3.address?.city);
console.log(u4?.address?.city);

// optional method call
const arr: number[] | null = [1, 2, 3];
const empty: number[] | null = null;
console.log(arr?.join(","));
console.log(empty?.join(","));

// optional index
const data: Record<string, number[]> | null = { keys: [1, 2] };
const noData: Record<string, number[]> | null = null;
console.log(data?.keys?.[0]);
console.log(noData?.keys?.[0]);

// optional + nullish coalescing
console.log(u3.address?.city ?? "unknown");
console.log(u1.address?.city ?? "unknown");

// optional chaining with function call
type Handler = { onEvent?: (x: number) => number };
const h1: Handler = { onEvent: (x) => x * 2 };
const h2: Handler = {};
console.log(h1.onEvent?.(5));
console.log(h2.onEvent?.(5));
