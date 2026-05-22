// Cross-runtime: Builder and fluent interface patterns.

class QueryBuilder {
  #table = "";
  #conditions: string[] = [];
  #columns: string[] = ["*"];
  #orderBy = "";
  #limit = 0;
  #offset = 0;

  from(table: string) { this.#table = table; return this; }
  select(...cols: string[]) { this.#columns = cols; return this; }
  where(cond: string) { this.#conditions.push(cond); return this; }
  order(col: string, dir: "ASC" | "DESC" = "ASC") { this.#orderBy = col + " " + dir; return this; }
  take(n: number) { this.#limit = n; return this; }
  skip(n: number) { this.#offset = n; return this; }

  build(): string {
    let q = "SELECT " + this.#columns.join(", ") + " FROM " + this.#table;
    if (this.#conditions.length) q += " WHERE " + this.#conditions.join(" AND ");
    if (this.#orderBy) q += " ORDER BY " + this.#orderBy;
    if (this.#limit) q += " LIMIT " + this.#limit;
    if (this.#offset) q += " OFFSET " + this.#offset;
    return q;
  }
}

const q1 = new QueryBuilder()
  .from("users")
  .select("id", "name", "email")
  .where("age > 18")
  .where("active = true")
  .order("name")
  .take(10)
  .skip(20)
  .build();
console.log(q1);

// Immutable builder (each method returns new instance)
class Vec3 {
  constructor(
    readonly x: number,
    readonly y: number,
    readonly z: number
  ) {}
  withX(x: number) { return new Vec3(x, this.y, this.z); }
  withY(y: number) { return new Vec3(this.x, y, this.z); }
  withZ(z: number) { return new Vec3(this.x, this.y, z); }
  add(other: Vec3) { return new Vec3(this.x + other.x, this.y + other.y, this.z + other.z); }
  scale(s: number) { return new Vec3(this.x * s, this.y * s, this.z * s); }
  get magnitude() { return Math.sqrt(this.x**2 + this.y**2 + this.z**2); }
  toString() { return "Vec3(" + this.x + "," + this.y + "," + this.z + ")"; }
}

const v = new Vec3(1, 2, 3).withX(10).scale(2).add(new Vec3(0, 1, 0));
console.log(v.toString());  // Vec3(20,5,6)
console.log(Math.round(new Vec3(3, 4, 0).magnitude));  // 5

// Promise-based builder (async query builder pattern)
class FetchBuilder {
  #url = "";
  #headers: Record<string, string> = {};
  #timeout = 5000;

  url(u: string) { this.#url = u; return this; }
  header(k: string, v: string) { this.#headers[k] = v; return this; }
  timeout(ms: number) { this.#timeout = ms; return this; }

  describe(): string {
    return "GET " + this.#url + " headers:" + Object.keys(this.#headers).join(",") + " timeout:" + this.#timeout;
  }
}

const req = new FetchBuilder()
  .url("https://api.example.com/users")
  .header("Authorization", "Bearer token")
  .header("Accept", "application/json")
  .timeout(3000);
console.log(req.describe());
