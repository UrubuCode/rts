// node:path, authored in TypeScript and compiled with the program.
//
// Replaces the native `crates/rts-node/src/path.rs`; behaviour follows Node's
// `lib/path.js` (checked against a real Node by differential test). The default
// export is the win32 or posix flavor by host platform (`rts:sys`), as Node's.

import { platform } from "rts:sys";

function received(value: any): string {
  if (value === undefined) return "Received undefined";
  if (value === null) return "Received null";
  if (typeof value === "function") return "Received function " + value.name;
  if (typeof value === "object") {
    const ctor = value.constructor;
    if (ctor && ctor.name) return "Received an instance of " + ctor.name;
    return "Received " + String(value);
  }
  let shown: string = typeof value === "string" ? value : String(value);
  if (typeof value === "string") {
    if (shown.length > 28) shown = shown.slice(0, 25) + "...";
    shown = "'" + shown + "'";
  } else if (typeof value === "bigint") {
    shown = shown + "n";
  }
  return "Received type " + typeof value + " (" + shown + ")";
}

function invalidType(name: string, expected: string, value: any): never {
  const error: any = new TypeError('The "' + name + '" argument must be ' + expected + ". " + received(value));
  error.code = "ERR_INVALID_ARG_TYPE";
  throw error;
}

function validateString(value: any, name: string): void {
  if (typeof value !== "string") invalidType(name, "of type string", value);
}

function validateObject(value: any, name: string): void {
  if (value === null || Array.isArray(value) || typeof value !== "object") {
    invalidType(name, "of type object", value);
  }
}

function isWindowsDeviceRoot(code: number): boolean {
  return (code >= 65 && code <= 90) || (code >= 97 && code <= 122);
}

function isSep(code: number, win: boolean): boolean {
  return code === 47 || (win && code === 92);
}

function normalizeString(path: string, allowAboveRoot: boolean, separator: string, win: boolean): string {
  let res = "";
  let lastSegmentLength = 0;
  let lastSlash = -1;
  let dots = 0;
  let code = 0;
  for (let i = 0; i <= path.length; ++i) {
    if (i < path.length) code = path.charCodeAt(i);
    else if (isSep(code, win)) break;
    else code = 47;
    if (isSep(code, win)) {
      if (lastSlash === i - 1 || dots === 1) {
        // nothing
      } else if (dots === 2) {
        if (res.length < 2 || lastSegmentLength !== 2 || res.charCodeAt(res.length - 1) !== 46 || res.charCodeAt(res.length - 2) !== 46) {
          if (res.length > 2) {
            const lastSlashIndex = res.lastIndexOf(separator);
            if (lastSlashIndex === -1) {
              res = "";
              lastSegmentLength = 0;
            } else {
              res = res.slice(0, lastSlashIndex);
              lastSegmentLength = res.length - 1 - res.lastIndexOf(separator);
            }
            lastSlash = i;
            dots = 0;
            continue;
          } else if (res.length !== 0) {
            res = "";
            lastSegmentLength = 0;
            lastSlash = i;
            dots = 0;
            continue;
          }
        }
        if (allowAboveRoot) {
          res += res.length > 0 ? separator + ".." : "..";
          lastSegmentLength = 2;
        }
      } else {
        if (res.length > 0) res += separator + path.slice(lastSlash + 1, i);
        else res = path.slice(lastSlash + 1, i);
        lastSegmentLength = i - lastSlash - 1;
      }
      lastSlash = i;
      dots = 0;
    } else if (code === 46 && dots !== -1) {
      ++dots;
    } else {
      dots = -1;
    }
  }
  return res;
}

function formatExt(ext: any): string {
  return ext ? (ext[0] === "." ? "" : ".") + ext : "";
}

function formatWith(sep: string, pathObject: any): string {
  validateObject(pathObject, "pathObject");
  const dir = pathObject.dir || pathObject.root;
  const base = pathObject.base || (pathObject.name || "") + formatExt(pathObject.ext);
  if (!dir) return base;
  return dir === pathObject.root ? dir + base : dir + sep + base;
}

function posixCwd(): string {
  if (platform() === "win32") {
    const cwd = process.cwd().split("\\").join("/");
    return cwd.slice(cwd.indexOf("/"));
  }
  return process.cwd();
}

// `*` (within a segment), `**` (across separators) and `?` only; brace
// expansion and character classes are not covered (same as the native it
// replaced).
function globMatches(path: string, pi: number, pattern: string, qi: number): boolean {
  while (true) {
    const atEnd = pi >= path.length;
    if (qi >= pattern.length) return atEnd;
    const want = pattern.charCodeAt(qi);
    if (want === 42) {
      const across = pattern.charCodeAt(qi + 1) === 42;
      const rest = qi + (across ? 2 : 1);
      if (atEnd) return rest >= pattern.length;
      if (globMatches(path, pi, pattern, rest)) return true;
      if (across || path.charCodeAt(pi) !== 47) {
        pi += 1;
        continue;
      }
      return false;
    }
    if (atEnd) return false;
    const here = path.charCodeAt(pi);
    if (want === 63 && here !== 47) {
      pi += 1;
      qi += 1;
      continue;
    }
    if (here === want) {
      pi += 1;
      qi += 1;
      continue;
    }
    return false;
  }
}

function matchesGlob(path: any, pattern: any): boolean {
  validateString(path, "path");
  validateString(pattern, "pattern");
  return globMatches(path, 0, pattern, 0);
}

function baseNameOf(path: string, suffix: any, win: boolean): string {
  if (suffix !== undefined) validateString(suffix, "suffix");
  validateString(path, "path");
  let start = 0;
  let end = -1;
  let matchedSlash = true;
  let i: number;
  if (win && path.length >= 2 && isWindowsDeviceRoot(path.charCodeAt(0)) && path.charCodeAt(1) === 58) start = 2;
  if (suffix !== undefined && suffix.length > 0 && suffix.length <= path.length) {
    if (suffix === path) return "";
    let extIdx = suffix.length - 1;
    let firstNonSlashEnd = -1;
    for (i = path.length - 1; i >= start; --i) {
      const code = path.charCodeAt(i);
      if (isSep(code, win)) {
        if (!matchedSlash) {
          start = i + 1;
          break;
        }
      } else {
        if (firstNonSlashEnd === -1) {
          matchedSlash = false;
          firstNonSlashEnd = i + 1;
        }
        if (extIdx >= 0) {
          if (code === suffix.charCodeAt(extIdx)) {
            if (--extIdx === -1) end = i;
          } else {
            extIdx = -1;
            end = firstNonSlashEnd;
          }
        }
      }
    }
    if (start === end) end = firstNonSlashEnd;
    else if (end === -1) end = path.length;
    return path.slice(start, end);
  }
  for (i = path.length - 1; i >= start; --i) {
    if (isSep(path.charCodeAt(i), win)) {
      if (!matchedSlash) {
        start = i + 1;
        break;
      }
    } else if (end === -1) {
      matchedSlash = false;
      end = i + 1;
    }
  }
  if (end === -1) return "";
  return path.slice(start, end);
}

function extNameOf(path: string, win: boolean): string {
  validateString(path, "path");
  let start = 0;
  let startDot = -1;
  let startPart = 0;
  let end = -1;
  let matchedSlash = true;
  let preDotState = 0;
  if (win && path.length >= 2 && path.charCodeAt(1) === 58 && isWindowsDeviceRoot(path.charCodeAt(0))) {
    start = 2;
    startPart = 2;
  }
  for (let i = path.length - 1; i >= start; --i) {
    const code = path.charCodeAt(i);
    if (isSep(code, win)) {
      if (!matchedSlash) {
        startPart = i + 1;
        break;
      }
      continue;
    }
    if (end === -1) {
      matchedSlash = false;
      end = i + 1;
    }
    if (code === 46) {
      if (startDot === -1) startDot = i;
      else if (preDotState !== 1) preDotState = 1;
    } else if (startDot !== -1) {
      preDotState = -1;
    }
  }
  if (startDot === -1 || end === -1 || preDotState === 0 || (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
    return "";
  }
  return path.slice(startDot, end);
}

// ---------------------------------------------------------------- win32

function win32Resolve(...args: any[]): string {
  let resolvedDevice = "";
  let resolvedTail = "";
  let resolvedAbsolute = false;
  for (let i = args.length - 1; i >= -1; i--) {
    let path: string;
    if (i >= 0) {
      path = args[i];
      validateString(path, "paths[" + i + "]");
      if (path.length === 0) continue;
    } else if (resolvedDevice.length === 0) {
      path = process.cwd();
    } else {
      path = process.env["=" + resolvedDevice] || process.cwd();
      if (path === undefined || (path.slice(0, 2).toLowerCase() !== resolvedDevice.toLowerCase() && path.charCodeAt(2) === 92)) {
        path = resolvedDevice + "\\";
      }
    }
    const len = path.length;
    let rootEnd = 0;
    let device = "";
    let isAbs = false;
    const code = path.charCodeAt(0);
    if (len === 1) {
      if (isSep(code, true)) {
        rootEnd = 1;
        isAbs = true;
      }
    } else if (isSep(code, true)) {
      isAbs = true;
      if (isSep(path.charCodeAt(1), true)) {
        let j = 2;
        let last = j;
        while (j < len && !isSep(path.charCodeAt(j), true)) j++;
        if (j < len && j !== last) {
          const firstPart = path.slice(last, j);
          last = j;
          while (j < len && isSep(path.charCodeAt(j), true)) j++;
          if (j < len && j !== last) {
            last = j;
            while (j < len && !isSep(path.charCodeAt(j), true)) j++;
            if (j === len || j !== last) {
              device = "\\\\" + firstPart + "\\" + path.slice(last, j);
              rootEnd = j;
            }
          }
        }
      } else {
        rootEnd = 1;
      }
    } else if (isWindowsDeviceRoot(code) && path.charCodeAt(1) === 58) {
      device = path.slice(0, 2);
      rootEnd = 2;
      if (len > 2 && isSep(path.charCodeAt(2), true)) {
        isAbs = true;
        rootEnd = 3;
      }
    }
    if (device.length > 0) {
      if (resolvedDevice.length > 0) {
        if (device.toLowerCase() !== resolvedDevice.toLowerCase()) continue;
      } else {
        resolvedDevice = device;
      }
    }
    if (resolvedAbsolute) {
      if (resolvedDevice.length > 0) break;
    } else {
      resolvedTail = path.slice(rootEnd) + "\\" + resolvedTail;
      resolvedAbsolute = isAbs;
      if (isAbs && resolvedDevice.length > 0) break;
    }
  }
  resolvedTail = normalizeString(resolvedTail, !resolvedAbsolute, "\\", true);
  return resolvedAbsolute ? resolvedDevice + "\\" + resolvedTail : resolvedDevice + resolvedTail || ".";
}

function win32Normalize(path: string): string {
  validateString(path, "path");
  const len = path.length;
  if (len === 0) return ".";
  let rootEnd = 0;
  let device: string | undefined = undefined;
  let isAbs = false;
  const code = path.charCodeAt(0);
  if (len === 1) {
    return code === 47 ? "\\" : path;
  }
  if (isSep(code, true)) {
    isAbs = true;
    if (isSep(path.charCodeAt(1), true)) {
      let j = 2;
      let last = j;
      while (j < len && !isSep(path.charCodeAt(j), true)) j++;
      if (j < len && j !== last) {
        const firstPart = path.slice(last, j);
        last = j;
        while (j < len && isSep(path.charCodeAt(j), true)) j++;
        if (j < len && j !== last) {
          last = j;
          while (j < len && !isSep(path.charCodeAt(j), true)) j++;
          if (j === len) {
            return "\\\\" + firstPart + "\\" + path.slice(last) + "\\";
          }
          if (j !== last) {
            device = "\\\\" + firstPart + "\\" + path.slice(last, j);
            rootEnd = j;
          }
        }
      }
    } else {
      rootEnd = 1;
    }
  } else if (isWindowsDeviceRoot(code) && path.charCodeAt(1) === 58) {
    device = path.slice(0, 2);
    rootEnd = 2;
    if (len > 2 && isSep(path.charCodeAt(2), true)) {
      isAbs = true;
      rootEnd = 3;
    }
  }
  let tail = rootEnd < len ? normalizeString(path.slice(rootEnd), !isAbs, "\\", true) : "";
  if (tail.length === 0 && !isAbs) tail = ".";
  if (tail.length > 0 && isSep(path.charCodeAt(len - 1), true)) tail += "\\";
  if (!isAbs && device === undefined && path.includes(":")) {
    if (tail.length >= 2 && isWindowsDeviceRoot(tail.charCodeAt(0)) && tail.charCodeAt(1) === 58) {
      return ".\\" + tail;
    }
    let index = path.indexOf(":");
    do {
      if (index === len - 1 || isSep(path.charCodeAt(index + 1), true)) {
        return ".\\" + tail;
      }
      index = path.indexOf(":", index + 1);
    } while (index !== -1);
  }
  if (device === undefined) return isAbs ? "\\" + tail : tail;
  return isAbs ? device + "\\" + tail : device + tail;
}

function win32IsAbsolute(path: string): boolean {
  validateString(path, "path");
  const len = path.length;
  if (len === 0) return false;
  const code = path.charCodeAt(0);
  return isSep(code, true) || (len > 2 && isWindowsDeviceRoot(code) && path.charCodeAt(1) === 58 && isSep(path.charCodeAt(2), true));
}

function win32Join(...args: any[]): string {
  if (args.length === 0) return ".";
  let joined: string | undefined = undefined;
  let firstPart = "";
  for (let i = 0; i < args.length; ++i) {
    const arg = args[i];
    validateString(arg, "path");
    if (arg.length > 0) {
      if (joined === undefined) {
        joined = arg;
        firstPart = arg;
      } else {
        joined += "\\" + arg;
      }
    }
  }
  if (joined === undefined) return ".";
  let needsReplace = true;
  let slashCount = 0;
  if (isSep(firstPart.charCodeAt(0), true)) {
    ++slashCount;
    const firstLen = firstPart.length;
    if (firstLen > 1 && isSep(firstPart.charCodeAt(1), true)) {
      ++slashCount;
      if (firstLen > 2) {
        if (isSep(firstPart.charCodeAt(2), true)) ++slashCount;
        else needsReplace = false;
      }
    }
  }
  if (needsReplace) {
    while (slashCount < joined.length && isSep(joined.charCodeAt(slashCount), true)) slashCount++;
    if (slashCount >= 2) joined = "\\" + joined.slice(slashCount);
  }
  return win32Normalize(joined);
}

function win32Relative(fromArg: string, toArg: string): string {
  validateString(fromArg, "from");
  validateString(toArg, "to");
  if (fromArg === toArg) return "";
  const fromOrig = win32Resolve(fromArg);
  const toOrig = win32Resolve(toArg);
  if (fromOrig === toOrig) return "";
  const from = fromOrig.toLowerCase();
  const to = toOrig.toLowerCase();
  if (from === to) return "";
  let fromStart = 0;
  while (fromStart < from.length && from.charCodeAt(fromStart) === 92) fromStart++;
  let fromEnd = from.length;
  while (fromEnd - 1 > fromStart && from.charCodeAt(fromEnd - 1) === 92) fromEnd--;
  const fromLen = fromEnd - fromStart;
  let toStart = 0;
  while (toStart < to.length && to.charCodeAt(toStart) === 92) toStart++;
  let toEnd = to.length;
  while (toEnd - 1 > toStart && to.charCodeAt(toEnd - 1) === 92) toEnd--;
  const toLen = toEnd - toStart;
  const length = fromLen < toLen ? fromLen : toLen;
  let lastCommonSep = -1;
  let i = 0;
  for (; i < length; i++) {
    const fromCode = from.charCodeAt(fromStart + i);
    if (fromCode !== to.charCodeAt(toStart + i)) break;
    else if (fromCode === 92) lastCommonSep = i;
  }
  if (i !== length) {
    if (lastCommonSep === -1) return toOrig;
  } else {
    if (toLen > length) {
      if (to.charCodeAt(toStart + i) === 92) {
        return toOrig.slice(toStart + i + 1);
      }
      if (i === 2) {
        return toOrig.slice(toStart + i);
      }
    }
    if (fromLen > length) {
      if (from.charCodeAt(fromStart + i) === 92) lastCommonSep = i;
      else if (i === 2) lastCommonSep = 3;
    }
    if (lastCommonSep === -1) lastCommonSep = 0;
  }
  let out = "";
  for (i = fromStart + lastCommonSep + 1; i <= fromEnd; ++i) {
    if (i === fromEnd || from.charCodeAt(i) === 92) {
      out += out.length === 0 ? ".." : "\\..";
    }
  }
  toStart += lastCommonSep;
  if (out.length > 0) return out + toOrig.slice(toStart, toEnd);
  if (toOrig.charCodeAt(toStart) === 92) ++toStart;
  return toOrig.slice(toStart, toEnd);
}

function win32ToNamespacedPath(path: any): any {
  if (typeof path !== "string" || path.length === 0) return path;
  const resolvedPath = win32Resolve(path);
  if (resolvedPath.length <= 2) return path;
  if (resolvedPath.charCodeAt(0) === 92) {
    if (resolvedPath.charCodeAt(1) === 92) {
      const code = resolvedPath.charCodeAt(2);
      if (code !== 63 && code !== 46) {
        return "\\\\?\\UNC\\" + resolvedPath.slice(2);
      }
    }
  } else if (isWindowsDeviceRoot(resolvedPath.charCodeAt(0)) && resolvedPath.charCodeAt(1) === 58 && resolvedPath.charCodeAt(2) === 92) {
    return "\\\\?\\" + resolvedPath;
  }
  return resolvedPath;
}

function win32Dirname(path: string): string {
  validateString(path, "path");
  const len = path.length;
  if (len === 0) return ".";
  let rootEnd = -1;
  let offset = 0;
  const code = path.charCodeAt(0);
  if (len === 1) {
    return isSep(code, true) ? path : ".";
  }
  if (isSep(code, true)) {
    rootEnd = offset = 1;
    if (isSep(path.charCodeAt(1), true)) {
      let j = 2;
      let last = j;
      while (j < len && !isSep(path.charCodeAt(j), true)) j++;
      if (j < len && j !== last) {
        last = j;
        while (j < len && isSep(path.charCodeAt(j), true)) j++;
        if (j < len && j !== last) {
          last = j;
          while (j < len && !isSep(path.charCodeAt(j), true)) j++;
          if (j === len) return path;
          if (j !== last) rootEnd = offset = j + 1;
        }
      }
    }
  } else if (isWindowsDeviceRoot(code) && path.charCodeAt(1) === 58) {
    rootEnd = len > 2 && isSep(path.charCodeAt(2), true) ? 3 : 2;
    offset = rootEnd;
  }
  let end = -1;
  let matchedSlash = true;
  for (let i = len - 1; i >= offset; --i) {
    if (isSep(path.charCodeAt(i), true)) {
      if (!matchedSlash) {
        end = i;
        break;
      }
    } else {
      matchedSlash = false;
    }
  }
  if (end === -1) {
    if (rootEnd === -1) return ".";
    end = rootEnd;
  }
  return path.slice(0, end);
}

function win32Parse(path: string): any {
  validateString(path, "path");
  const ret: any = { root: "", dir: "", base: "", ext: "", name: "" };
  if (path.length === 0) return ret;
  const len = path.length;
  let rootEnd = 0;
  let code = path.charCodeAt(0);
  if (len === 1) {
    if (isSep(code, true)) {
      ret.root = ret.dir = path;
      return ret;
    }
    ret.base = ret.name = path;
    return ret;
  }
  if (isSep(code, true)) {
    rootEnd = 1;
    if (isSep(path.charCodeAt(1), true)) {
      let j = 2;
      let last = j;
      while (j < len && !isSep(path.charCodeAt(j), true)) j++;
      if (j < len && j !== last) {
        last = j;
        while (j < len && isSep(path.charCodeAt(j), true)) j++;
        if (j < len && j !== last) {
          last = j;
          while (j < len && !isSep(path.charCodeAt(j), true)) j++;
          if (j === len) rootEnd = j;
          else if (j !== last) rootEnd = j + 1;
        }
      }
    }
  } else if (isWindowsDeviceRoot(code) && path.charCodeAt(1) === 58) {
    if (len <= 2) {
      ret.root = ret.dir = path;
      return ret;
    }
    rootEnd = 2;
    if (isSep(path.charCodeAt(2), true)) {
      if (len === 3) {
        ret.root = ret.dir = path;
        return ret;
      }
      rootEnd = 3;
    }
  }
  if (rootEnd > 0) ret.root = path.slice(0, rootEnd);
  let startDot = -1;
  let startPart = rootEnd;
  let end = -1;
  let matchedSlash = true;
  let i = path.length - 1;
  let preDotState = 0;
  for (; i >= rootEnd; --i) {
    code = path.charCodeAt(i);
    if (isSep(code, true)) {
      if (!matchedSlash) {
        startPart = i + 1;
        break;
      }
      continue;
    }
    if (end === -1) {
      matchedSlash = false;
      end = i + 1;
    }
    if (code === 46) {
      if (startDot === -1) startDot = i;
      else if (preDotState !== 1) preDotState = 1;
    } else if (startDot !== -1) {
      preDotState = -1;
    }
  }
  if (end !== -1) {
    if (startDot === -1 || preDotState === 0 || (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
      ret.base = ret.name = path.slice(startPart, end);
    } else {
      ret.name = path.slice(startPart, startDot);
      ret.base = path.slice(startPart, end);
      ret.ext = path.slice(startDot, end);
    }
  }
  if (startPart > 0 && startPart !== rootEnd) ret.dir = path.slice(0, startPart - 1);
  else ret.dir = ret.root;
  return ret;
}

// ---------------------------------------------------------------- posix

function posixResolve(...args: any[]): string {
  let resolvedPath = "";
  let resolvedAbsolute = false;
  for (let i = args.length - 1; i >= 0 && !resolvedAbsolute; i--) {
    const path = args[i];
    validateString(path, "paths[" + i + "]");
    if (path.length === 0) continue;
    resolvedPath = path + "/" + resolvedPath;
    resolvedAbsolute = path.charCodeAt(0) === 47;
  }
  if (!resolvedAbsolute) {
    const cwd = posixCwd();
    resolvedPath = cwd + "/" + resolvedPath;
    resolvedAbsolute = cwd.charCodeAt(0) === 47;
  }
  resolvedPath = normalizeString(resolvedPath, !resolvedAbsolute, "/", false);
  if (resolvedAbsolute) return "/" + resolvedPath;
  return resolvedPath.length > 0 ? resolvedPath : ".";
}

function posixNormalize(path: string): string {
  validateString(path, "path");
  if (path.length === 0) return ".";
  const isAbs = path.charCodeAt(0) === 47;
  const trailing = path.charCodeAt(path.length - 1) === 47;
  path = normalizeString(path, !isAbs, "/", false);
  if (path.length === 0) {
    if (isAbs) return "/";
    return trailing ? "./" : ".";
  }
  if (trailing) path += "/";
  return isAbs ? "/" + path : path;
}

function posixIsAbsolute(path: string): boolean {
  validateString(path, "path");
  return path.length > 0 && path.charCodeAt(0) === 47;
}

function posixJoin(...args: any[]): string {
  if (args.length === 0) return ".";
  let joined: string | undefined = undefined;
  for (let i = 0; i < args.length; ++i) {
    const arg = args[i];
    validateString(arg, "path");
    if (arg.length > 0) {
      if (joined === undefined) joined = arg;
      else joined += "/" + arg;
    }
  }
  if (joined === undefined) return ".";
  return posixNormalize(joined);
}

function posixRelative(fromArg: string, toArg: string): string {
  validateString(fromArg, "from");
  validateString(toArg, "to");
  if (fromArg === toArg) return "";
  const from = posixResolve(fromArg);
  const to = posixResolve(toArg);
  if (from === to) return "";
  const fromStart = 1;
  const fromEnd = from.length;
  const fromLen = fromEnd - fromStart;
  const toStart = 1;
  const toLen = to.length - toStart;
  const length = fromLen < toLen ? fromLen : toLen;
  let lastCommonSep = -1;
  let i = 0;
  for (; i < length; i++) {
    const fromCode = from.charCodeAt(fromStart + i);
    if (fromCode !== to.charCodeAt(toStart + i)) break;
    else if (fromCode === 47) lastCommonSep = i;
  }
  if (i === length) {
    if (toLen > length) {
      if (to.charCodeAt(toStart + i) === 47) {
        return to.slice(toStart + i + 1);
      }
      if (i === 0) {
        return to.slice(toStart + i);
      }
    } else if (fromLen > length) {
      if (from.charCodeAt(fromStart + i) === 47) lastCommonSep = i;
      else if (i === 0) lastCommonSep = 0;
    }
  }
  let out = "";
  for (i = fromStart + lastCommonSep + 1; i <= fromEnd; ++i) {
    if (i === fromEnd || from.charCodeAt(i) === 47) {
      out += out.length === 0 ? ".." : "/..";
    }
  }
  return out + to.slice(toStart + lastCommonSep);
}

function posixToNamespacedPath(path: any): any {
  return path;
}

function posixDirname(path: string): string {
  validateString(path, "path");
  if (path.length === 0) return ".";
  const hasRoot = path.charCodeAt(0) === 47;
  let end = -1;
  let matchedSlash = true;
  for (let i = path.length - 1; i >= 1; --i) {
    if (path.charCodeAt(i) === 47) {
      if (!matchedSlash) {
        end = i;
        break;
      }
    } else {
      matchedSlash = false;
    }
  }
  if (end === -1) return hasRoot ? "/" : ".";
  if (hasRoot && end === 1) return "//";
  return path.slice(0, end);
}

function posixParse(path: string): any {
  validateString(path, "path");
  const ret: any = { root: "", dir: "", base: "", ext: "", name: "" };
  if (path.length === 0) return ret;
  const isAbs = path.charCodeAt(0) === 47;
  let start: number;
  if (isAbs) {
    ret.root = "/";
    start = 1;
  } else {
    start = 0;
  }
  let startDot = -1;
  let startPart = 0;
  let end = -1;
  let matchedSlash = true;
  let i = path.length - 1;
  let preDotState = 0;
  for (; i >= start; --i) {
    const code = path.charCodeAt(i);
    if (code === 47) {
      if (!matchedSlash) {
        startPart = i + 1;
        break;
      }
      continue;
    }
    if (end === -1) {
      matchedSlash = false;
      end = i + 1;
    }
    if (code === 46) {
      if (startDot === -1) startDot = i;
      else if (preDotState !== 1) preDotState = 1;
    } else if (startDot !== -1) {
      preDotState = -1;
    }
  }
  if (end !== -1) {
    const from = startPart === 0 && isAbs ? 1 : startPart;
    if (startDot === -1 || preDotState === 0 || (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
      ret.base = ret.name = path.slice(from, end);
    } else {
      ret.name = path.slice(from, startDot);
      ret.base = path.slice(from, end);
      ret.ext = path.slice(startDot, end);
    }
  }
  if (startPart > 0) ret.dir = path.slice(0, startPart - 1);
  else if (isAbs) ret.dir = "/";
  return ret;
}

// ------------------------------------------------------------- flavors

export const win32: any = {
  resolve: win32Resolve,
  normalize: win32Normalize,
  isAbsolute: win32IsAbsolute,
  join: win32Join,
  relative: win32Relative,
  toNamespacedPath: win32ToNamespacedPath,
  dirname: win32Dirname,
  basename: (path: any, suffix?: any): string => baseNameOf(path, suffix, true),
  extname: (path: any): string => extNameOf(path, true),
  format: (pathObject: any): string => formatWith("\\", pathObject),
  parse: win32Parse,
  matchesGlob: matchesGlob,
  sep: "\\",
  delimiter: ";",
  win32: null,
  posix: null,
};

export const posix: any = {
  resolve: posixResolve,
  normalize: posixNormalize,
  isAbsolute: posixIsAbsolute,
  join: posixJoin,
  relative: posixRelative,
  toNamespacedPath: posixToNamespacedPath,
  dirname: posixDirname,
  basename: (path: any, suffix?: any): string => baseNameOf(path, suffix, false),
  extname: (path: any): string => extNameOf(path, false),
  format: (pathObject: any): string => formatWith("/", pathObject),
  parse: posixParse,
  matchesGlob: matchesGlob,
  sep: "/",
  delimiter: ":",
  win32: null,
  posix: null,
};

posix.win32 = win32.win32 = win32;
posix.posix = win32.posix = posix;

const current: any = platform() === "win32" ? win32 : posix;

export const resolve = current.resolve;
export const normalize = current.normalize;
export const isAbsolute = current.isAbsolute;
export const join = current.join;
export const relative = current.relative;
export const toNamespacedPath = current.toNamespacedPath;
export const dirname = current.dirname;
export const basename = current.basename;
export const extname = current.extname;
export const format = current.format;
export const parse = current.parse;
export const sep: string = current.sep;
export const delimiter: string = current.delimiter;
export { matchesGlob };

export default current;
