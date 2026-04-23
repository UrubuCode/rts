import { String } from "./string";
export { String } from "./string";

export function install(): void {
  globalThis.String = String;
  global.String = String;
}

install();
