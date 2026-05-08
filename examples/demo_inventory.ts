import { time } from "rts";

// Simulação de inventário com pedidos.
// Usa __Flat prefix para que classe tenha layout nativo e preserve
// tipos f64 em fields. Sem isso, fields number sao armazenados via
// MAP_SET que coage para i64 (perde fracao).

class __FlatItem {
  name: string = "";
  price: number = 0;
  stock: i32 = 0;
  constructor(n: string, p: number, s: i32) {
    this.name = n;
    this.price = p;
    this.stock = s;
  }
  total(qty: i32): number { return this.price * qty; }
}

const apple = new __FlatItem("Apple", 1.50, 100);
const bread = new __FlatItem("Bread", 3.99, 50);
const cheese = new __FlatItem("Cheese", 7.25, 30);
const drink = new __FlatItem("Drink", 2.10, 80);

const catalog: __FlatItem[] = [apple, bread, cheese, drink];

console.log("=== Inventory ===");
for (const it of catalog) {
  console.log(`${it.name}: $${it.price.toFixed(2)} (stock: ${it.stock})`);
}

// Calcular totais por (item, qty) sem array de items
function lineTotal(it: __FlatItem, qty: i32): number {
  return it.total(qty);
}

console.log("\n=== Order Calculation ===");
const o1Total = lineTotal(apple, 5) + lineTotal(bread, 2);
const o2Total = lineTotal(cheese, 3) + lineTotal(drink, 6);
const o3Total = lineTotal(apple, 10) + lineTotal(cheese, 1) + lineTotal(drink, 2);

console.log(`Order #1: $${o1Total.toFixed(2)}`);
console.log(`Order #2: $${o2Total.toFixed(2)}`);
console.log(`Order #3: $${o3Total.toFixed(2)}`);

const grand = o1Total + o2Total + o3Total;
const count = 3;
console.log(`\nGrand total: $${grand.toFixed(2)}`);
console.log(`Average: $${(grand / count).toFixed(2)}`);

// Map de revenue por item
const revenue = new Map<string, number>();
revenue.set(apple.name, lineTotal(apple, 5) + lineTotal(apple, 10));
revenue.set(bread.name, lineTotal(bread, 2));
revenue.set(cheese.name, lineTotal(cheese, 3) + lineTotal(cheese, 1));
revenue.set(drink.name, lineTotal(drink, 6) + lineTotal(drink, 2));

console.log("\n=== Revenue by item ===");
for (const [name, rev] of revenue) {
  console.log(`${name}: $${rev.toFixed(2)}`);
}

// Array methods
const prices: number[] = [apple.price, bread.price, cheese.price, drink.price];
console.log(`\nPrices: ${prices.join(", ")}`);
console.log(`Min: $${Math.min(prices[0], prices[1], prices[2], prices[3]).toFixed(2)}`);
console.log(`Max: $${Math.max(prices[0], prices[1], prices[2], prices[3]).toFixed(2)}`);

// JSON
const summary = {
  total: grand,
  count: count,
  items: catalog.length,
};
console.log(`\nJSON: ${JSON.stringify(summary)}`);

// Bench loop 100k
const t0 = time.now_ms();
let s = 0;
for (let i = 0; i < 100000; i = i + 1) s = s + i;
const t1 = time.now_ms();
console.log(`\nLoop 100k: sum=${s}, took ${t1 - t0}ms`);

// Regex
const text = "Order #1: Apple x5; Order #2: Cheese x3";
const m = text.match(/Order #\d+/);
console.log(`\nRegex match: ${m}`);
const replaced = text.replace(/Order/g, "Pedido");
console.log(`Replaced: ${replaced}`);

// Optional chaining
type Cfg = { server?: { port?: number } };
const cfg: Cfg = { server: { port: 8080 } };
const empty: Cfg = {};
console.log(`\nServer port: ${cfg?.server?.port ?? "default"}`);
console.log(`Empty port: ${empty?.server?.port ?? "default"}`);

// Symbol
const tag = Symbol("orderTag");
console.log(`\nSymbol: ${tag.toString()}`);
console.log(`typeof: ${typeof tag}`);

// Object literal getter/setter
const counter = {
  _n: 0,
  get value() { return this._n; },
  set value(v: number) { this._n = v * 2; }
};
counter.value = 21;
console.log(`\nCounter (set 21, get -> ${counter.value})`);

console.log("\n=== Done ===");
