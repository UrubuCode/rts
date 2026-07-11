// node:path/win32 — full-surface parity test (native port of Node's win32.js).
import { describe, test, expect } from "rts:test";
import {
    basename,
    isAbsolute,
    join,
    normalize,
    relative,
    parse,
    toNamespacedPath,
    sep,
    delimiter,
} from "node:path/win32";

const bn1 = basename("C:\\foo.HTML", ".html"); // case mismatch → unchanged
const bn2 = basename("C:\\temp\\myfile.html");
const ab1 = isAbsolute("//server");
const ab2 = isAbsolute("\\\\server");
const ab3 = isAbsolute("C:/foo/..");
const ab4 = isAbsolute("C:\\foo\\..");
const ab5 = isAbsolute("bar\\baz");
const ab6 = isAbsolute(".");
const ab7 = isAbsolute("C:foo");
const jn1 = join("C:\\", "foo", "..\\bar");
const nm1 = normalize("C:\\temp\\\\foo\\bar\\..\\");
const nm2 = normalize("C:////temp\\\\/\\/\\/foo/bar");
const p = parse("C:\\path\\dir\\file.txt");
const rl1 = relative("C:\\orandea\\test\\aaa", "C:\\orandea\\impl\\bbb");
const tn1 = toNamespacedPath("C:\\foo\\bar");
const sepV = sep;
const delimV = delimiter;

describe("node:path/win32", () => {
    test("basename case suffix", () => expect(bn1).toBe("foo.HTML"));
    test("basename", () => expect(bn2).toBe("myfile.html"));
    test("isAbsolute //server", () => expect(ab1).toBe(true));
    test("isAbsolute UNC", () => expect(ab2).toBe(true));
    test("isAbsolute C:/", () => expect(ab3).toBe(true));
    test("isAbsolute C:\\", () => expect(ab4).toBe(true));
    test("isAbsolute rel", () => expect(ab5).toBe(false));
    test("isAbsolute dot", () => expect(ab6).toBe(false));
    test("isAbsolute drive-rel", () => expect(ab7).toBe(false));
    test("join", () => expect(jn1).toBe("C:\\bar"));
    test("normalize trailing", () => expect(nm1).toBe("C:\\temp\\foo\\"));
    test("normalize mixed seps", () => expect(nm2).toBe("C:\\temp\\foo\\bar"));
    test("parse root", () => expect(p.root).toBe("C:\\"));
    test("parse dir", () => expect(p.dir).toBe("C:\\path\\dir"));
    test("parse base", () => expect(p.base).toBe("file.txt"));
    test("parse ext", () => expect(p.ext).toBe(".txt"));
    test("parse name", () => expect(p.name).toBe("file"));
    test("relative", () => expect(rl1).toBe("..\\..\\impl\\bbb"));
    test("toNamespacedPath", () => expect(tn1).toBe("\\\\?\\C:\\foo\\bar"));
    test("sep", () => expect(sepV).toBe("\\"));
    test("delimiter", () => expect(delimV).toBe(";"));
});
