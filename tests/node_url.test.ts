import { describe, test, expect } from "rts:test";
import {
  fileURLToPath, pathToFileURL, domainToASCII, domainToUnicode,
  urlToHttpOptions, format, parse, resolve as urlResolve,
} from "node:url";

// ---- URL instance (ambient global) ----------------------------------------
const u: any = new URL("https://user:pass@h.com:8443/p/q?x=1&y=2#frag");
const href = u.href;
const parts = [u.protocol, u.username, u.password, u.hostname, u.port, u.host, u.pathname, u.search, u.hash, u.origin].join("|");
const toJson = u.toJSON();
const toStr = u.toString();

// ---- URL + searchParams mutation sync -------------------------------------
const u2: any = new URL("https://h.com/p?x=1");
u2.searchParams.append("z", "3");
const searchAfter = u2.search;
const hrefAfter = u2.href;

// ---- URLSearchParams full surface -----------------------------------------
const sp: any = new URLSearchParams("a=1&b=2&a=3");
const spGet = sp.get("a");
const spGetAll = JSON.stringify(sp.getAll("a"));
const spHas = sp.has("b");
const spKeys = JSON.stringify(sp.keys());
const spValues = JSON.stringify(sp.values());
const spSize = sp.size;
const spEntries = JSON.stringify(sp.entries());
const spForEach: any[] = [];
sp.forEach((v: any, k: any) => { spForEach.push(k + "=" + v); });
sp.append("c", "9");
sp.set("a", "X");
const spSetToString = sp.toString();
sp.delete("b");
const spAfterDelete = sp.toString();
const sp2: any = new URLSearchParams("c=3&a=1&b=2");
sp2.sort();
const spSorted = sp2.toString();

// ---- node:url functions ----------------------------------------------------
const fPath = fileURLToPath("file:///C:/a/b");
const pURL = pathToFileURL("/a/b").href;
const dAscii = domainToASCII("mañana.com");
const dUni = domainToUnicode("xn--maana-pta.com");
const httpOpts: any = urlToHttpOptions(new URL("http://x.com:81/p?a=1"));
const fmtOut = format(new URL("http://x.com/p?a=1"));
const legacy: any = parse("http://h.com:8/p?x=1");
const resolved = urlResolve("http://a.com/b/c", "../d");

describe("node:url", () => {
  test("URL instance properties", () => {
    expect(href).toBe("https://user:pass@h.com:8443/p/q?x=1&y=2#frag");
    expect(parts).toBe("https:|user|pass|h.com|8443|h.com:8443|/p/q|?x=1&y=2|#frag|https://h.com:8443");
    expect(toJson).toBe(href);
    expect(toStr).toBe(href);
  });
  test("searchParams mutation reflects in search/href", () => {
    expect(searchAfter).toBe("?x=1&z=3");
    expect(hrefAfter).toBe("https://h.com/p?x=1&z=3");
  });
  test("URLSearchParams multimap surface", () => {
    expect(spGet).toBe("1");
    expect(spGetAll).toBe("[\"1\",\"3\"]");
    expect(spHas).toBe(true);
    expect(spKeys).toBe("[\"a\",\"b\",\"a\"]");
    expect(spValues).toBe("[\"1\",\"2\",\"3\"]");
    expect(spSize).toBe(3);
    expect(spEntries).toBe("[[\"a\",\"1\"],[\"b\",\"2\"],[\"a\",\"3\"]]");
    expect(spForEach.join(",")).toBe("a=1,b=2,a=3");
  });
  test("URLSearchParams set/append/delete/sort", () => {
    expect(spSetToString).toBe("a=X&b=2&c=9");
    expect(spAfterDelete).toBe("a=X&c=9");
    expect(spSorted).toBe("a=1&b=2&c=3");
  });
  test("node:url functions", () => {
    expect(fPath).toBe("C:\\a\\b");
    expect(pURL).toBe("file:///a/b");
    expect(dAscii).toBe("xn--maana-pta.com");
    expect(dUni).toBe("mañana.com");
    expect(httpOpts.hostname).toBe("x.com");
    expect("" + httpOpts.port).toBe("81");
    expect(fmtOut).toBe("http://x.com/p?a=1");
    expect(legacy.hostname).toBe("h.com");
    expect(legacy.port).toBe("8");
    expect(resolved).toBe("http://a.com/d");
  });
});
