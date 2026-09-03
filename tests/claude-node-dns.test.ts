// node:dns — API shape, construction and synchronous-throw contracts.
// NO network I/O in this file on purpose (see the companion `-net` file for
// real resolution): every assertion here is either a `typeof` check, a
// constructor/array shape, or a throw that is decided by argument
// validation alone, before any query is issued. Every "Node says" value
// below was confirmed against a real Node v20.19.5 on this machine with
// `node -e "..."` — quoted inline where it matters.
import { describe, test, expect } from "rts:test";
import dns, { Resolver } from "node:dns";

// ---- 1. every documented member exists as a function -----------------
const fnNames = [
    "resolve", "resolve4", "resolve6", "resolveAny", "resolveCaa", "resolveCname",
    "resolveMx", "resolveNaptr", "resolveNs", "resolvePtr", "resolveSoa",
    "resolveSrv", "resolveTxt", "reverse", "lookup", "getServers", "setServers",
    "getDefaultResultOrder", "setDefaultResultOrder",
] as const;
const allFunctionsOk = fnNames.every((name) => typeof (dns as any)[name] === "function");

// `resolveTlsa` is implemented here but does NOT exist on this machine's
// installed Node v20.19.5 (`Object.keys(require('dns'))` has no `resolveTlsa`
// at all — confirmed, not merely undocumented). So this is RTS's own added
// surface rather than something to compare against Node; asserted on its
// own, not folded into `allFunctionsOk` above.
const resolveTlsaIsFunction = typeof dns.resolveTlsa === "function";

const resolverCtorOk = typeof Resolver === "function";

// ---- 2. dns.Resolver is a real, constructible class -------------------
const r = new Resolver();
const resolverInstanceOk = r instanceof Resolver;
const resolverMethodsOk = ["resolve", "resolve4", "resolve6", "reverse", "getServers", "setServers", "setLocalAddress", "cancel"].every(
    (name) => typeof (r as any)[name] === "function",
);
const resolverGetServersIsArray = Array.isArray(r.getServers());
// `cancel()` is a documented honest no-op here (no outstanding query can
// ever exist given this crate's synchronous-from-the-caller contract) —
// real Node's `cancel()` also returns `undefined`.
const cancelReturnsUndefined = r.cancel() === undefined;

// ---- 3. Resolver#setServers throws on a malformed entry ---------------
// Node: `new Resolver().setServers(['not-an-ip'])` -> TypeError,
// code ERR_INVALID_IP_ADDRESS, message "Invalid IP address: not-an-ip".
// This one MATCHES: RTS raises the identical TypeError/code.
let instanceSetServersThrew = false;
let instanceSetServersCode = "";
let instanceSetServersIsTypeError = false;
try {
    new Resolver().setServers(["not-an-ip"]);
} catch (e: any) {
    instanceSetServersThrew = true;
    instanceSetServersCode = e.code;
    instanceSetServersIsTypeError = e instanceof TypeError;
}

// A NUMBER entry in the array: Node raises ERR_INVALID_ARG_TYPE ("servers[0]"
// argument must be of type string). RTS's `array_texts` stringifies the
// number first (`text_of(123)` -> "123") and only THEN tries to parse it as
// an IP, so it raises ERR_INVALID_IP_ADDRESS instead — same throw, wrong
// code. Confirmed on both sides; asserting Node's code here.
let instanceSetServersNumericCode = "";
try {
    new Resolver().setServers([123 as unknown as string]);
} catch (e: any) {
    instanceSetServersNumericCode = e.code;
}

// ---- 4. dns.setServers (MODULE level) — Node throws, RTS silently no-ops
// Node: `dns.setServers(['not-an-ip'])` throws TypeError ERR_INVALID_IP_ADDRESS
// synchronously (confirmed with `node -e`) — the exact same contract the
// `Resolver` instance method has, just documented differently in this
// crate's `state.rs`, whose module doc claims "this module has no way to
// raise a catchable exception... matching Node's void return". That claim
// is FALSE (measured): Node's module-level `setServers` throws exactly like
// the class method, and `mod.rs`'s own doc already notes elsewhere that a
// real catchable error became available here after `state.rs` was written
// ("Errors are plain objects... — except one throw"). `state.rs` was never
// updated to use it, so a malformed entry today is silently swallowed: the
// server list is left unchanged and the call returns `undefined`, same as
// a VALID call would look from the caller's side.
let moduleSetServersThrew = false;
try {
    dns.setServers(["not-an-ip"]);
} catch {
    moduleSetServersThrew = true;
}

// ---- 5. dns.reverse on a malformed IP: Node's real code is EINVAL, NOT
// ERR_INVALID_IP_ADDRESS. Measured: `dns.reverse('not-an-ip', cb)` throws a
// plain `Error` (NOT a `TypeError`) with `code: 'EINVAL'` and
// `syscall: 'getHostByAddr'`, message "getHostByAddr EINVAL not-an-ip".
// `rr_alias.rs`'s module doc claims this "match[es] Node's documented
// contract" for `ERR_INVALID_IP_ADDRESS` — also false, measured the same
// way. RTS raises `TypeError [ERR_INVALID_IP_ADDRESS]` instead (verified
// below by construction, since the two codes are mutually exclusive).
let reverseThrew = false;
let reverseCode = "";
let reverseSyscall = "";
let reverseIsPlainError = false;
try {
    dns.reverse("not-an-ip", () => {});
} catch (e: any) {
    reverseThrew = true;
    reverseCode = e.code;
    reverseSyscall = e.syscall;
    reverseIsPlainError = !(e instanceof TypeError) && e instanceof Error;
}

// Same measured divergence on `Resolver#reverse` — Node: `Error`, EINVAL,
// syscall `getHostByAddr`.
let resolverReverseCode = "";
try {
    new Resolver().reverse("not-an-ip", () => {});
} catch (e: any) {
    resolverReverseCode = e.code;
}

// A non-string `ip` (Node validates the TYPE before the FORMAT): Node
// throws `ERR_INVALID_ARG_TYPE` ("The \"name\" argument must be of type
// string. Received type number (123)"), not EINVAL and not
// ERR_INVALID_IP_ADDRESS.
let reverseNumericCode = "";
try {
    dns.reverse(123 as unknown as string, () => {});
} catch (e: any) {
    reverseNumericCode = e.code;
}

// ---- 6. dns.resolveSoa exists and is not undefined (matches Node) -----
const resolveSoaOk = typeof dns.resolveSoa === "function" && dns.resolveSoa !== undefined;

// ---- 7. dns.ADDRCONFIG / V4MAPPED / ALL — Node's REAL numeric values ---
// Node: ADDRCONFIG=1024, V4MAPPED=2048, ALL=256 (measured with `node -e`).
// These are documented here as "inert bookkeeping" (never consulted by
// `lookup`), which is a legitimate design choice — but the VALUES chosen
// (4, 8, 16) do not match Node's at all, so a program doing its own
// bitmasking against them (`hints & dns.ADDRCONFIG`) gets a number that
// means something different than it does under Node, inert or not.
const addrconfigOk = dns.ADDRCONFIG === 1024;
const v4mappedOk = dns.V4MAPPED === 2048;
const allFlagOk = dns.ALL === 256;

// ---- 8. dns.resolve(hostname, callback) — the 2-arg form, defaulting
// rrtype to 'A' — Node: this ALWAYS passes argument validation (there is
// nothing invalid about omitting rrtype; it is documented to default to
// 'A') and goes on to actually query. So whatever the callback's `err` ends
// up being, its `code` can never be `ERR_INVALID_ARG_VALUE` — that code is
// reserved for an UNRECOGNIZED rrtype string, and omitting the argument
// entirely is not one. This assertion needs no network to be true of Node:
// it is a fact about argument validation, checked before any query is
// issued — which is also why the RTS bug below reproduces with zero
// network I/O (`dispatch`'s `_ => Err("ERR_INVALID_ARG_VALUE")` arm fires
// before the resolver is ever asked to look anything up).
//
// Measured on RTS: the `rrtype` slot detection in `resolve()` sets
// `kind = entry::text_of(rrtype).unwrap_or_else(|| "A".to_owned())`, meant
// to default to "A" when `rrtype` is the "argument not supplied" sentinel —
// but the callback's `err.code` comes back `ERR_INVALID_ARG_VALUE` every
// time the rrtype argument is omitted, meaning `kind` is NOT ending up as
// the string "A" as the code assumes. This is the two-argument form of
// `dns.resolve` and `Resolver#resolve` — arguably the MOST common call
// shape of this function — entirely broken, unconditionally, independent
// of hostname or network reachability.
let twoArgResolveErr: any = null;
dns.resolve("example.com", (err: any) => {
    twoArgResolveErr = err;
});
const twoArgResolveWronglyFails = twoArgResolveErr !== null && twoArgResolveErr.code === "ERR_INVALID_ARG_VALUE";

let twoArgResolverInstanceErr: any = null;
new Resolver().resolve("example.com", (err: any) => {
    twoArgResolverInstanceErr = err;
});
const twoArgResolverInstanceWronglyFails = twoArgResolverInstanceErr !== null && twoArgResolverInstanceErr.code === "ERR_INVALID_ARG_VALUE";

// Explicit "A" (a real string, not the "argument absent" sentinel) DOES
// work — isolates the defect to the DEFAULTING path specifically, not to
// `dispatch`'s "A" arm itself.
let explicitAErr: any = null;
let explicitARes: any = null;
dns.resolve("example.com", "A", (err: any, res: any) => {
    explicitAErr = err;
    explicitARes = res;
});
const explicitAWorks = explicitAErr === null && Array.isArray(explicitARes) && explicitARes.length > 0;

// ---- 9. dns.resolve with an unrecognized rrtype: Node throws
// SYNCHRONOUSLY (`TypeError [ERR_INVALID_ARG_VALUE]`, "The argument
// 'rrtype' is invalid. Received 'BOGUS'") — the query never starts and the
// callback is never invoked. `resolve.rs`'s own doc acknowledges this
// crate answers through the callback instead, with a stated reason
// ("cannot raise THAT specific catchable error from a value it hasn't
// looked at") that `mod.rs`'s doc says is now stale — a real catchable
// `ERR_INVALID_ARG_VALUE` is exactly what `crate::errors` already raises
// elsewhere in this same file's sibling modules (`invalid_ip_address`).
let bogusRrtypeThrew = false;
let bogusRrtypeCallbackFired = false;
try {
    dns.resolve("example.com", "BOGUS", () => {
        bogusRrtypeCallbackFired = true;
    });
} catch {
    bogusRrtypeThrew = true;
}

// ---- 10. dns.promises.resolve4 — Node DOES have it; this crate's own doc
// (`mod.rs`, "Two resolution paths" / "Not implemented, by name") states
// PLAINLY and correctly that it is deliberately withheld ("this task's test
// corpus only imports resolve4 from node:dns... a dns.promises.resolve4
// that nothing exercises is exactly the unverified surface reuse-check
// exists to keep out") — unlike the two "matches Node" claims measured
// false above, this one names the gap honestly and gives a real reason.
// Still asserting Node's actual answer per this file's own rule.
const promisesResolve4IsFunction = typeof dns.promises.resolve4 === "function";

// ---- 11. getDefaultResultOrder / setDefaultResultOrder round-trip -----
const orderBefore = dns.getDefaultResultOrder();
dns.setDefaultResultOrder("ipv4first");
const orderAfter = dns.getDefaultResultOrder();
const orderRoundTripOk = orderBefore === "verbatim" && orderAfter === "ipv4first";

describe("node:dns — shape, construction, throws (no network)", () => {
    test("every documented resolve*/reverse/lookup/getServers/setServers member is a function", () => expect(allFunctionsOk).toBe(true));
    test("resolveTlsa is a function (RTS-only surface: absent from this machine's Node v20.19.5 entirely)", () => expect(resolveTlsaIsFunction).toBe(true));
    test("dns.Resolver is a constructor function", () => expect(resolverCtorOk).toBe(true));
    test("new Resolver() is instanceof Resolver", () => expect(resolverInstanceOk).toBe(true));
    test("Resolver instance has every documented method", () => expect(resolverMethodsOk).toBe(true));
    test("Resolver#getServers() returns an array", () => expect(resolverGetServersIsArray).toBe(true));
    test("Resolver#cancel() returns undefined (honest no-op, matches Node)", () => expect(cancelReturnsUndefined).toBe(true));

    test("Resolver#setServers(['not-an-ip']) throws TypeError [ERR_INVALID_IP_ADDRESS] (matches Node)", () => {
        expect(instanceSetServersThrew).toBe(true);
        expect(instanceSetServersCode).toBe("ERR_INVALID_IP_ADDRESS");
        expect(instanceSetServersIsTypeError).toBe(true);
    });
    test("Resolver#setServers([123]) — Node says ERR_INVALID_ARG_TYPE, RTS says ERR_INVALID_IP_ADDRESS (RED: known divergence)", () => {
        expect(instanceSetServersNumericCode).toBe("ERR_INVALID_ARG_TYPE");
    });

    test("dns.setServers(['not-an-ip']) throws (module level) — Node throws, RTS silently no-ops (RED: doc claims parity, measured false)", () => {
        expect(moduleSetServersThrew).toBe(true);
    });

    test("dns.reverse(malformed) — Node: Error/EINVAL/getHostByAddr, not ERR_INVALID_IP_ADDRESS (RED: doc claims parity, measured false)", () => {
        expect(reverseThrew).toBe(true);
        expect(reverseCode).toBe("EINVAL");
        expect(reverseSyscall).toBe("getHostByAddr");
        expect(reverseIsPlainError).toBe(true);
    });
    test("Resolver#reverse(malformed) — same EINVAL contract as the module function (RED)", () => expect(resolverReverseCode).toBe("EINVAL"));
    test("dns.reverse(123) — non-string arg: Node says ERR_INVALID_ARG_TYPE (RED: RTS says ERR_INVALID_IP_ADDRESS)", () => expect(reverseNumericCode).toBe("ERR_INVALID_ARG_TYPE"));

    test("dns.resolveSoa exists and is not undefined", () => expect(resolveSoaOk).toBe(true));

    test("dns.ADDRCONFIG === 1024 (RED: RTS defines it as 4)", () => expect(addrconfigOk).toBe(true));
    test("dns.V4MAPPED === 2048 (RED: RTS defines it as 8)", () => expect(v4mappedOk).toBe(true));
    test("dns.ALL === 256 (RED: RTS defines it as 16)", () => expect(allFlagOk).toBe(true));

    test("dns.resolve(hostname, callback) 2-arg form never fails with ERR_INVALID_ARG_VALUE (RED: RTS always does, no network needed to prove it)", () => {
        expect(twoArgResolveWronglyFails).toBe(false);
    });
    test("Resolver#resolve(hostname, callback) 2-arg form — same defect (RED)", () => expect(twoArgResolverInstanceWronglyFails).toBe(false));
    test("dns.resolve(hostname, 'A', callback) — explicit rrtype DOES work (isolates the defect to defaulting)", () => expect(explicitAWorks).toBe(true));

    test("dns.resolve(host, 'BOGUS', cb) throws synchronously TypeError [ERR_INVALID_ARG_VALUE] (RED: RTS answers via callback instead)", () => {
        expect(bogusRrtypeThrew).toBe(true);
        expect(bogusRrtypeCallbackFired).toBe(false);
    });

    test("dns.promises.resolve4 is a function (RED: deliberately withheld here — see comment, not a surprise)", () => {
        expect(promisesResolve4IsFunction).toBe(true);
    });

    test("getDefaultResultOrder/setDefaultResultOrder round-trip (verbatim -> ipv4first)", () => expect(orderRoundTripOk).toBe(true));
});
