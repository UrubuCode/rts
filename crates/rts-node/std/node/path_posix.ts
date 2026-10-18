// node:path/posix — the posix flavor of the TypeScript `node:path`.
import { posix } from "node:path";

export const resolve = posix.resolve;
export const normalize = posix.normalize;
export const isAbsolute = posix.isAbsolute;
export const join = posix.join;
export const relative = posix.relative;
export const toNamespacedPath = posix.toNamespacedPath;
export const dirname = posix.dirname;
export const basename = posix.basename;
export const extname = posix.extname;
export const format = posix.format;
export const parse = posix.parse;
export const matchesGlob = posix.matchesGlob;
export const sep: string = posix.sep;
export const delimiter: string = posix.delimiter;
export { posix };
export const win32 = posix.win32;

export default posix;
