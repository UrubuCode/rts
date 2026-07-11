// node:path/posix — full-surface parity test (native port of Node's posix.js).
import { describe, test, expect } from "rts:test";
import {
    basename,
    dirname,
    extname,
    isAbsolute,
    join,
    normalize,
    relative,
    resolve,
    parse,
    format,
    toNamespacedPath,
    matchesGlob,
    sep,
    delimiter,
} from "node:path/posix";

const bn1 = basename("/foo/bar/baz/asdf/quux.html");
const bn2 = basename("/foo/bar/baz/asdf/quux.html", ".html");
const bn3 = basename("/foo/bar///");
const dn1 = dirname("/foo/bar/baz/asdf/quux");
const dn2 = dirname("foo");
const en1 = extname("index.html");
const en2 = extname("index.coffee.md");
const en3 = extname("index.");
const en4 = extname("index");
const en5 = extname(".index");
const en6 = extname(".index.md");
const ab1 = isAbsolute("/foo/bar");
const ab2 = isAbsolute("qux/");
const ab3 = isAbsolute(".");
const ab4 = isAbsolute("");
const jn1 = join("/foo", "bar", "baz/asdf", "quux", "..");
const jn2 = join();
const jn3 = join("");
const nm1 = normalize("/foo/bar//baz/asdf/quux/..");
const nm2 = normalize("a/b/./c/../../d");
const nm3 = normalize("");
const rl1 = relative("/data/orandea/test/aaa", "/data/orandea/impl/bbb");
const rl2 = relative("/a/b", "/a/b");
const rs1 = resolve("/foo/bar", "./baz");
const rs2 = resolve("/foo/bar", "/tmp/file/");
const p = parse("/home/user/dir/file.txt");
const fm1 = format({ root: "/", dir: "/home/user/dir", base: "file.txt" });
const fm2 = format({ root: "/", name: "file", ext: "txt" });
const fm3 = format({ root: "/", name: "file", ext: ".txt" });
const tn1 = toNamespacedPath("/foo/bar");
const gl1 = matchesGlob("/foo/bar", "/foo/*");
const gl2 = matchesGlob("/foo/bar*", "foo/bird");
const gl3 = matchesGlob("a/b/c", "a/**/c");
const sepV = sep;
const delimV = delimiter;

describe("node:path/posix", () => {
    test("basename", () => expect(bn1).toBe("quux.html"));
    test("basename suffix", () => expect(bn2).toBe("quux"));
    test("basename trailing sep", () => expect(bn3).toBe("bar"));
    test("dirname", () => expect(dn1).toBe("/foo/bar/baz/asdf"));
    test("dirname relative", () => expect(dn2).toBe("."));
    test("extname html", () => expect(en1).toBe(".html"));
    test("extname double", () => expect(en2).toBe(".md"));
    test("extname trailing dot", () => expect(en3).toBe("."));
    test("extname none", () => expect(en4).toBe(""));
    test("extname hidden", () => expect(en5).toBe(""));
    test("extname hidden+ext", () => expect(en6).toBe(".md"));
    test("isAbsolute true", () => expect(ab1).toBe(true));
    test("isAbsolute rel", () => expect(ab2).toBe(false));
    test("isAbsolute dot", () => expect(ab3).toBe(false));
    test("isAbsolute empty", () => expect(ab4).toBe(false));
    test("join", () => expect(jn1).toBe("/foo/bar/baz/asdf"));
    test("join empty", () => expect(jn2).toBe("."));
    test("join blank", () => expect(jn3).toBe("."));
    test("normalize", () => expect(nm1).toBe("/foo/bar/baz/asdf"));
    test("normalize dots", () => expect(nm2).toBe("a/d"));
    test("normalize empty", () => expect(nm3).toBe("."));
    test("relative", () => expect(rl1).toBe("../../impl/bbb"));
    test("relative same", () => expect(rl2).toBe(""));
    test("resolve rel", () => expect(rs1).toBe("/foo/bar/baz"));
    test("resolve abs", () => expect(rs2).toBe("/tmp/file"));
    test("parse root", () => expect(p.root).toBe("/"));
    test("parse dir", () => expect(p.dir).toBe("/home/user/dir"));
    test("parse base", () => expect(p.base).toBe("file.txt"));
    test("parse ext", () => expect(p.ext).toBe(".txt"));
    test("parse name", () => expect(p.name).toBe("file"));
    test("format dir wins", () => expect(fm1).toBe("/home/user/dir/file.txt"));
    test("format dot auto", () => expect(fm2).toBe("/file.txt"));
    test("format ext dot", () => expect(fm3).toBe("/file.txt"));
    test("toNamespacedPath noop", () => expect(tn1).toBe("/foo/bar"));
    test("matchesGlob star", () => expect(gl1).toBe(true));
    test("matchesGlob no", () => expect(gl2).toBe(false));
    test("matchesGlob globstar", () => expect(gl3).toBe(true));
    test("sep", () => expect(sepV).toBe("/"));
    test("delimiter", () => expect(delimV).toBe(":"));
});
