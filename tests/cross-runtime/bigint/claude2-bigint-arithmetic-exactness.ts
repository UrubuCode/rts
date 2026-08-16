// ONE thing: BigInt arithmetic is EXACT at any size, which is the whole point
// of the type. Every digit below is pinned — a factorial, a Fibonacci, a power
// of two with 300 digits — and the same computation in Number loses precision
// at 2^53, which the comparison makes explicit.

// --- factorial: exact where Number stops being exact at 18! ---
function factorial(n: bigint): bigint {
  let acc = 1n;
  for (let i = 2n; i <= n; i++) acc = acc * i;
  return acc;
}
console.log("20!=" + String(factorial(20n)));
console.log("25!=" + String(factorial(25n)));
console.log("30!=" + String(factorial(30n)));
console.log("50!=" + String(factorial(50n)));
console.log("50!_digits=" + factorial(50n).toString().length);
console.log("100!_digits=" + factorial(100n).toString().length);
console.log("100!_trailing_zeros=" + (factorial(100n).toString().length - factorial(100n).toString().replace(/0+$/, "").length));

// --- and the Number version diverges exactly where the double gives out ---
function numberFactorial(n: number): number {
  let acc = 1;
  for (let i = 2; i <= n; i++) acc = acc * i;
  return acc;
}
const firstLoss: string[] = [];
for (let n = 1; n <= 25; n++) {
  const exact = factorial(BigInt(n));
  if (BigInt(numberFactorial(n)) !== exact) {
    firstLoss.push(String(n));
  }
}
console.log("number_factorial_first_wrong=" + firstLoss[0] + " all_wrong_from=" + firstLoss.join(","));
console.log("18!_number=" + String(numberFactorial(18)) + " exact=" + String(factorial(18n)));
console.log("21!_number=" + String(numberFactorial(21)) + " exact=" + String(factorial(21n)));

// --- Fibonacci, iterated with a destructuring swap ---
function fib(n: number): bigint {
  let a = 0n;
  let b = 1n;
  for (let i = 0; i < n; i++) {
    const t = a + b;
    a = b;
    b = t;
  }
  return a;
}
console.log("fib_50=" + String(fib(50)));
console.log("fib_100=" + String(fib(100)));
console.log("fib_200=" + String(fib(200)));
console.log("fib_300_digits=" + fib(300).toString().length);
console.log("fib_identity=" + (fib(100) === fib(99) + fib(98)));

// --- powers of two, exactly, well past the double's range ---
console.log("2^64=" + String(2n ** 64n));
console.log("2^64-1=" + String(2n ** 64n - 1n));
console.log("2^100=" + String(2n ** 100n));
console.log("2^1024_digits=" + (2n ** 1024n).toString().length);
console.log("2^4096_digits=" + (2n ** 4096n).toString().length);
console.log("2^64_is_shift=" + ((1n << 64n) === 2n ** 64n));
console.log("2^1024_finite=" + Number.isFinite(Number(2n ** 1024n)) + " as_number=" + String(Number(2n ** 1024n)));

// --- the safe-integer frontier, from both sides ---
const safe = BigInt(Number.MAX_SAFE_INTEGER);
console.log("max_safe=" + String(safe));
console.log("max_safe_plus_1=" + String(safe + 1n));
console.log("max_safe_plus_2=" + String(safe + 2n));
console.log("number_plus_1=" + String(Number.MAX_SAFE_INTEGER + 1));
console.log("number_plus_2=" + String(Number.MAX_SAFE_INTEGER + 2));
console.log("number_loses_it=" + (Number.MAX_SAFE_INTEGER + 1 === Number.MAX_SAFE_INTEGER + 2));
console.log("bigint_keeps_it=" + (safe + 1n === safe + 2n));
console.log("roundtrip_through_number=" + (BigInt(Number(safe + 2n)) === safe + 2n));
console.log("BigInt_of_2p53=" + String(BigInt(2 ** 53)) + " vs " + String(2n ** 53n));

// --- exact division only when it divides: the remainder is dropped ---
console.log("div_exact=" + String(100n / 5n));
console.log("div_truncates=" + String(7n / 2n) + "," + String(-7n / 2n));
console.log("div_reconstruct=" + ((7n / 2n) * 2n + (7n % 2n) === 7n));
console.log("div_reconstruct_neg=" + ((-7n / 2n) * 2n + (-7n % 2n) === -7n));
console.log("big_div=" + String(factorial(30n) / factorial(28n)));
console.log("big_mod=" + String(factorial(30n) % 1000000007n));

// --- modular exponentiation, the classic reason to want exactness ---
function powMod(base: bigint, exp: bigint, mod: bigint): bigint {
  let result = 1n;
  let b = base % mod;
  let e = exp;
  while (e > 0n) {
    if (e % 2n === 1n) result = (result * b) % mod;
    b = (b * b) % mod;
    e = e / 2n;
  }
  return result;
}
console.log("powmod_small=" + String(powMod(2n, 10n, 1000n)));
console.log("powmod_fermat=" + String(powMod(2n, 1000000006n, 1000000007n)));
console.log("powmod_big=" + String(powMod(3n, 200n, 1000000007n)));
console.log("powmod_matches_direct=" + (powMod(3n, 20n, 97n) === 3n ** 20n % 97n));

// --- a greatest common divisor over values no double could hold ---
function gcd(a: bigint, b: bigint): bigint {
  let x = a < 0n ? -a : a;
  let y = b < 0n ? -b : b;
  while (y !== 0n) {
    const t = x % y;
    x = y;
    y = t;
  }
  return x;
}
console.log("gcd=" + String(gcd(factorial(20n), factorial(15n))));
console.log("gcd_coprime=" + String(gcd(2n ** 61n - 1n, 2n ** 31n - 1n)));
console.log("gcd_zero=" + String(gcd(0n, 5n)) + "," + String(gcd(5n, 0n)));

// --- comparison and ordering stay exact across the frontier ---
const values: bigint[] = [safe + 2n, safe + 1n, safe, -safe, 0n, 2n ** 100n, -(2n ** 100n)];
console.log("sorted=" + values.slice().sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).map((v) => String(v)).join(","));
console.log("max_by_reduce=" + String(values.reduce((a, b) => (a > b ? a : b))));
console.log("strict_ordering=" + (safe + 1n < safe + 2n) + " number_ordering=" + (Number.MAX_SAFE_INTEGER + 1 < Number.MAX_SAFE_INTEGER + 2));
