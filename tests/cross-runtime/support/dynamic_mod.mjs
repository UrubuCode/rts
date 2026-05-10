export const namedValue = 17;

export const state = { count: 0 };
state.count += 1;

export default function makeValue(prefix) {
  return prefix + ":" + namedValue;
}
