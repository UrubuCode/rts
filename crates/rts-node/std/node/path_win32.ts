// node:path/win32 — the win32 flavor of the TypeScript `node:path`.
import { win32 } from "node:path";

export const resolve = win32.resolve;
export const normalize = win32.normalize;
export const isAbsolute = win32.isAbsolute;
export const join = win32.join;
export const relative = win32.relative;
export const toNamespacedPath = win32.toNamespacedPath;
export const dirname = win32.dirname;
export const basename = win32.basename;
export const extname = win32.extname;
export const format = win32.format;
export const parse = win32.parse;
export const matchesGlob = win32.matchesGlob;
export const sep: string = win32.sep;
export const delimiter: string = win32.delimiter;
export { win32 };
export const posix = win32.posix;

export default win32;
