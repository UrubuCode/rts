// Support module for 100_dynamic_import (dynamic import + module caching).
export const namedValue = 7;
export default function (x) {
  return "default(" + x + ")";
}
export const state = { count: 42 };
