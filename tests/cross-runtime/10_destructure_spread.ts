// Cross-runtime: destructuring, defaults, rest params and spread.
const coords = [10, 20, 30, 40];
const [first, second = 0, third, ...rest] = coords;
console.log("arr_pick=" + [first, second, third].join(","));
console.log("arr_rest=" + rest.join(","));

const cfg = { host: "localhost", port: 8080, secure: "off", mode: "dev" };
const { host, port, missing = "fallback", ...cfgRest } = cfg;
console.log("obj_pick=" + host + ":" + port + ":" + missing);
console.log("obj_rest_keys=" + Object.keys(cfgRest).join(","));
console.log("obj_rest_vals=" + [cfgRest.secure, cfgRest.mode].join(","));

function sum3(a: number, b: number, c: number): number {
  return a + b + c;
}

function collect(label: string, ...values: number[]): string {
  return label + ":" + values.join("|") + ":" + values.length;
}

const nums = [4, 5, 6];
console.log("spread_call=" + sum3(...nums));
console.log("rest_call=" + collect("n", 1, 2, 3, 4));

const merged = { base: 1, ...{ extra: 2 }, base: 3, tail: 4 };
console.log("merged=" + JSON.stringify(merged));

const clone = [...coords.slice(0, 2), 99, ...coords.slice(2)];
console.log("clone=" + clone.join(","));
