// node:url — function-surface parity test (IDNA / file: / legacy).
import { describe, test, expect } from "rts:test";
import {
    domainToASCII,
    domainToUnicode,
    fileURLToPath,
    pathToFileURL,
    urlToHttpOptions,
    resolve,
    parse,
    format,
} from "node:url";
import { platform } from "node:os";

// --- IDNA -------------------------------------------------------------------
const a1 = domainToASCII("español.com");
const a2 = domainToASCII("中文.com");
const u1 = domainToUnicode("xn--espaol-zwa.com");

// --- file: URL conversion ---------------------------------------------------
//
// `fileURLToPath`/`pathToFileURL` are PLATFORM-DEPENDENT in Node: a drive-letter
// URL yields `C:\foo\bar.txt` on win32 and `/C:/foo/bar.txt` on POSIX, and a
// backslash is a legal filename character on POSIX, not a separator.
//
// This block used to hardcode the win32 answers with the comment "dev/CI runner
// is win32". That was false — the CI matrix runs ubuntu and macos as well, where
// the runtime returned the correct POSIX answers and the test failed for being
// right. Input AND expectation are now chosen per platform, so both sides assert
// the real Node semantics instead of one platform's.
const isWin = platform() === "win32";

const fileUrl = isWin ? "file:///C:/foo/bar.txt" : "file:///foo/bar.txt";
const fileUrlPct = isWin ? "file:///C:/foo%20bar" : "file:///foo%20bar";
const nativePath = isWin ? "C:\\dir\\file.txt" : "/dir/file.txt";
const nativeHash = isWin ? "C:\\foo#1" : "/foo#1";

const expPath = isWin ? "C:\\foo\\bar.txt" : "/foo/bar.txt";
const expPathPct = isWin ? "C:\\foo bar" : "/foo bar";
const expHref = isWin ? "file:///C:/dir/file.txt" : "file:///dir/file.txt";
const expHrefHash = isWin ? "file:///C:/foo%231" : "file:///foo%231";

const fp1 = fileURLToPath(fileUrl);
const fp2 = fileURLToPath(fileUrlPct); // %20 → space
const pfuA = pathToFileURL(nativePath);
const pfu1 = pfuA.href;
const pfuB = pathToFileURL(nativeHash);
const pfu2 = pfuB.href; // # encoded

// --- urlToHttpOptions -------------------------------------------------------
const opts = urlToHttpOptions(new URL("https://user:pass@example.com:8080/p?q=1#h"));
const optHost = opts.hostname;
const optPort = opts.port;
const optPath = opts.path;
const optProto = opts.protocol;
const optAuth = opts.auth;

// --- resolve ----------------------------------------------------------------
const r1 = resolve("http://example.com/a/b", "c"); // → .../a/c
const r2 = resolve("http://example.com/a/b", "/d"); // → .../d

// --- legacy parse -----------------------------------------------------------
const p1 = parse("http://user@host.com:81/p/q?x=1#frag");
const p1proto = p1.protocol;
const p1host = p1.host;
const p1hostname = p1.hostname;
const p1port = p1.port;
const p1auth = p1.auth;
const p1path = p1.pathname;
const p1search = p1.search;
const p1hash = p1.hash;

// --- legacy format (roundtrip a subset) -------------------------------------
const f1 = format({
    protocol: "http:",
    slashes: true,
    host: "host.com:81",
    pathname: "/p",
    search: "?x=1",
    hash: "#h",
});

describe("node:url function surface", () => {
    test("domainToASCII spanish", () => expect(a1).toBe("xn--espaol-zwa.com"));
    test("domainToASCII chinese", () => expect(a2).toBe("xn--fiq228c.com"));
    test("domainToUnicode", () => expect(u1).toBe("español.com"));
    test("fileURLToPath", () => expect(fp1).toBe(expPath));
    test("fileURLToPath decode", () => expect(fp2).toBe(expPathPct));
    test("pathToFileURL href", () => expect(pfu1).toBe(expHref));
    test("pathToFileURL encodes hash", () => expect(pfu2).toBe(expHrefHash));
    test("urlToHttpOptions hostname", () => expect(optHost).toBe("example.com"));
    test("urlToHttpOptions port", () => expect(optPort).toBe(8080));
    test("urlToHttpOptions path", () => expect(optPath).toBe("/p?q=1"));
    test("urlToHttpOptions protocol", () => expect(optProto).toBe("https:"));
    test("urlToHttpOptions auth", () => expect(optAuth).toBe("user:pass"));
    test("resolve relative", () => expect(r1).toBe("http://example.com/a/c"));
    test("resolve absolute path", () => expect(r2).toBe("http://example.com/d"));
    test("parse protocol", () => expect(p1proto).toBe("http:"));
    test("parse host", () => expect(p1host).toBe("host.com:81"));
    test("parse hostname", () => expect(p1hostname).toBe("host.com"));
    test("parse port", () => expect(p1port).toBe("81"));
    test("parse auth", () => expect(p1auth).toBe("user"));
    test("parse pathname", () => expect(p1path).toBe("/p/q"));
    test("parse search", () => expect(p1search).toBe("?x=1"));
    test("parse hash", () => expect(p1hash).toBe("#frag"));
    test("format recompose", () => expect(f1).toBe("http://host.com:81/p?x=1#h"));
});
