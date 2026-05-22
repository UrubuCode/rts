// Cross-runtime: DCE patterns used by bundlers.
// Bundlers use these patterns to tree-shake modules.

// Pattern 1: constant condition (bundler folds at build time)
const IS_PROD = true;
if (IS_PROD) {
  console.log("prod");
} else {
  console.log("dev");  // dead in prod bundle
}

// Pattern 2: short-circuit evaluation for feature flags
const FEATURE_X = false;
FEATURE_X && console.log("feature-x enabled");  // never runs
!FEATURE_X && console.log("feature-x disabled");

// Pattern 3: typeof guard (UMD bundler pattern)
const hasWindow = typeof globalThis !== "undefined";
console.log(hasWindow);

// Pattern 4: __DEV__ style constant branch
const __DEV__ = false;
function warn(msg: string) {
  if (__DEV__) console.log("[warn] " + msg);
}
warn("this should not print");
console.log("no warning above");

// Pattern 5: object spread for module re-export (bundler output)
const base = { a: 1, b: 2 };
const extended = { ...base, c: 3 };
console.log(extended.a, extended.b, extended.c);

// Pattern 6: default export interop pattern
const mod = { default: 42, __esModule: true };
const val = mod.__esModule ? mod.default : mod;
console.log(val);
