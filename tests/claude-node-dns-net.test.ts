// node:dns — real DNS-protocol resolution, over the network.
// Every domain below was chosen for a stable RR type it is known to
// publish (verified with a real `node -e` query against the same live
// network first) and every shape assertion mirrors what that query
// actually returned — not what would be convenient. If this machine has no
// route to the network, every test in this file fails the same way
// (ENOTFOUND/timeout on every query): that is an environment fact, not an
// engine defect, and is called out at the bottom of the report rather than
// filed as one.
import { describe, test, expect } from "rts:test";
import dns, { Resolver } from "node:dns";

function collect<T>(run: (cb: (err: any, res: T) => void) => void): { err: any; res: T | null } {
    let out: { err: any; res: T | null } = { err: "not-called", res: null };
    run((err, res) => {
        out = { err, res };
    });
    return out;
}

// resolve4 — real Node: ["104.20.23.154","172.66.147.243"] (order varies).
const a = collect<string[]>((cb) => dns.resolve4("example.com", cb));
const resolve4Ok = a.err === null && Array.isArray(a.res) && a.res!.length > 0 && a.res!.every((s) => /^\d+\.\d+\.\d+\.\d+$/.test(s));

// resolve6 — google.com always publishes AAAA.
const aaaa = collect<string[]>((cb) => dns.resolve6("google.com", cb));
const resolve6Ok = aaaa.err === null && Array.isArray(aaaa.res) && aaaa.res!.length > 0 && aaaa.res!.every((s) => s.includes(":"));

// resolve(host, callback) — the 2-arg default-to-'A' form. See the
// companion no-network file for why this is expected to be RED here too:
// the defect fires before any query is even issued, so it reproduces with
// or without a real network path.
const def = collect<string[]>((cb) => dns.resolve("example.com", cb));
const resolveDefaultOk = def.err === null && Array.isArray(def.res) && def.res!.length > 0;

// resolveMx — real Node: [{ priority: 10, exchange: "smtp.google.com" }].
const mx = collect<Array<{ priority: number; exchange: string }>>((cb) => dns.resolveMx("google.com", cb));
const mxOk = mx.err === null && Array.isArray(mx.res) && mx.res!.length > 0 && typeof mx.res![0].priority === "number" && typeof mx.res![0].exchange === "string";

// resolveTxt — Node: `string[][]`, ONE INNER ARRAY PER TXT RECORD (not a
// flat array of strings — the shape this module's own doc calls out).
const txt = collect<string[][]>((cb) => dns.resolveTxt("google.com", cb));
const txtOk = txt.err === null && Array.isArray(txt.res) && txt.res!.length > 0 && Array.isArray(txt.res![0]) && typeof txt.res![0][0] === "string";

// resolveCname — real Node: www.github.com -> ["github.com"], no trailing dot.
const cname = collect<string[]>((cb) => dns.resolveCname("www.github.com", cb));
const cnameOk = cname.err === null && Array.isArray(cname.res) && cname.res!.length > 0 && cname.res!.every((s) => !s.endsWith("."));

// resolveNs — real Node: google.com's own NS set, 4 entries.
const ns = collect<string[]>((cb) => dns.resolveNs("google.com", cb));
const nsOk = ns.err === null && Array.isArray(ns.res) && ns.res!.length > 0 && ns.res!.every((s) => s.includes("."));

// resolveSoa — a SINGLE object (a zone has exactly one SOA), not an array.
const soa = collect<{ nsname: string; hostmaster: string; serial: number; refresh: number; retry: number; expire: number; minttl: number }>((cb) =>
    dns.resolveSoa("google.com", cb),
);
const soaOk =
    soa.err === null &&
    !Array.isArray(soa.res) &&
    typeof soa.res!.nsname === "string" &&
    typeof soa.res!.hostmaster === "string" &&
    typeof soa.res!.serial === "number" &&
    typeof soa.res!.refresh === "number";

// resolveCaa — real Node: [{ critical: 0, issue: "pki.goog" }] for google.com
// — `critical` is the raw wire flags BYTE (0 or 128), not a boolean, and the
// value is keyed by its own tag name ("issue"/"issuewild"/"iodef"/...).
const caa = collect<Array<{ critical: number; issue?: string }>>((cb) => dns.resolveCaa("google.com", cb));
const caaOk = caa.err === null && Array.isArray(caa.res) && caa.res!.length > 0 && typeof caa.res![0].critical === "number" && (caa.res![0].critical === 0 || caa.res![0].critical === 128);

// resolveSrv — real Node: google.com's own CalDAV SRV record.
const srv = collect<Array<{ priority: number; weight: number; port: number; name: string }>>((cb) => dns.resolveSrv("_caldav._tcp.google.com", cb));
const srvOk =
    srv.err === null &&
    Array.isArray(srv.res) &&
    srv.res!.length > 0 &&
    typeof srv.res![0].priority === "number" &&
    typeof srv.res![0].weight === "number" &&
    typeof srv.res![0].port === "number" &&
    typeof srv.res![0].name === "string";

// resolveNaptr — sip2sip.info publishes real NAPTR records for its SIP
// service discovery.
const naptr = collect<Array<{ flags: string; service: string; regexp: string; replacement: string; order: number; preference: number }>>((cb) =>
    dns.resolveNaptr("sip2sip.info", cb),
);
const naptrOk = naptr.err === null && Array.isArray(naptr.res) && naptr.res!.length > 0 && typeof naptr.res![0].service === "string" && naptr.res![0].service.indexOf("SIP") === 0;

// reverse — real Node: 8.8.8.8 -> ["dns.google"].
const rev = collect<string[]>((cb) => dns.reverse("8.8.8.8", cb));
const reverseOk = rev.err === null && Array.isArray(rev.res) && rev.res!.indexOf("dns.google") !== -1;

// resolveAny — tagged union, `type` names which shape each element has.
const any = collect<Array<{ type: string }>>((cb) => dns.resolveAny("google.com", cb));
const anyOk = any.err === null && Array.isArray(any.res) && any.res!.length > 0 && any.res!.every((r) => typeof r.type === "string") && any.res!.some((r) => r.type === "A");

// dns.Resolver — the SAME queries, over an independent instance. Proves
// the class does real per-instance resolution and is not merely a second
// name for the module-level functions.
const instance = new Resolver();
const instMx = collect<Array<{ priority: number; exchange: string }>>((cb) => instance.resolveMx("google.com", cb));
const instanceResolverWorksOk = instMx.err === null && Array.isArray(instMx.res) && instMx.res!.length > 0;

// resolveTlsa — RTS-only surface (this Node v20.19.5 does not have this
// function at all, so there is no Node answer to compare against). Only
// asserting RTS does not crash and answers a plausible shape: real DANE
// records for the IETF mail server, `{certUsage, selector, match, data}`
// with `data` a Buffer.
const tlsa = collect<Array<{ certUsage: number; selector: number; match: number; data: Uint8Array }>>((cb) => dns.resolveTlsa("_25._tcp.mail.ietf.org", cb));
const tlsaShapeOk =
    tlsa.err === null &&
    Array.isArray(tlsa.res) &&
    tlsa.res!.length > 0 &&
    typeof tlsa.res![0].certUsage === "number" &&
    typeof tlsa.res![0].selector === "number" &&
    typeof tlsa.res![0].match === "number" &&
    tlsa.res![0].data instanceof Uint8Array;

describe("node:dns — real resolution over the network", () => {
    test("resolve4('example.com') -> dotted-quad strings", () => expect(resolve4Ok).toBe(true));
    test("resolve6('google.com') -> colon-form strings", () => expect(resolve6Ok).toBe(true));
    test("resolve(host, cb) 2-arg default-to-'A' actually resolves (RED, see no-network file)", () => expect(resolveDefaultOk).toBe(true));
    test("resolveMx('google.com') -> [{priority, exchange}]", () => expect(mxOk).toBe(true));
    test("resolveTxt('google.com') -> string[][] (one inner array per TXT record)", () => expect(txtOk).toBe(true));
    test("resolveCname('www.github.com') -> hostnames, no trailing dot", () => expect(cnameOk).toBe(true));
    test("resolveNs('google.com') -> nameserver hostnames", () => expect(nsOk).toBe(true));
    test("resolveSoa('google.com') -> a single object, not an array", () => expect(soaOk).toBe(true));
    test("resolveCaa('google.com') -> critical is the raw flags byte, value keyed by tag", () => expect(caaOk).toBe(true));
    test("resolveSrv('_caldav._tcp.google.com') -> {priority, weight, port, name}", () => expect(srvOk).toBe(true));
    test("resolveNaptr('sip2sip.info') -> DDDS fields", () => expect(naptrOk).toBe(true));
    test("reverse('8.8.8.8') -> ['dns.google']", () => expect(reverseOk).toBe(true));
    test("resolveAny('google.com') -> tagged union, includes an 'A' entry", () => expect(anyOk).toBe(true));
    test("dns.Resolver instance resolves independently of the module-level functions", () => expect(instanceResolverWorksOk).toBe(true));
    test("resolveTlsa — RTS-only surface, plausible shape (no Node ground truth on this machine)", () => expect(tlsaShapeOk).toBe(true));
});
