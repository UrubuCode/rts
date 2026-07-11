// node:os — full-surface parity smoke test (real syscalls, no fixed values).
import { describe, test, expect } from "rts:test";
import {
    platform,
    arch,
    type,
    endianness,
    machine,
    release,
    version,
    hostname,
    homedir,
    tmpdir,
    totalmem,
    freemem,
    uptime,
    loadavg,
    availableParallelism,
    cpus,
    networkInterfaces,
    userInfo,
    getPriority,
    EOL,
    devNull,
    constants,
} from "node:os";

// --- identity ---------------------------------------------------------------
const plat = platform();
const platOk = plat === "win32" || plat === "linux" || plat === "darwin" ||
    plat === "freebsd" || plat === "openbsd" || plat === "android";
const archStr = arch();
const archOk = archStr.length > 0;
const typeOk = type().length > 0;
const end = endianness();
const endOk = end === "LE" || end === "BE";
const machineOk = machine().length > 0;
const releaseOk = release().length > 0;
const versionOk = version().length > 0;
const hostOk = hostname().length > 0;
const homeOk = homedir().length > 0;
const tmpStr = tmpdir();
const tmpOk = tmpStr.length > 0 &&
    tmpStr[tmpStr.length - 1] !== "/" && tmpStr[tmpStr.length - 1] !== "\\";

// --- memory / uptime / load -------------------------------------------------
const total = totalmem();
const free = freemem();
const memOk = total > 0 && free > 0 && free <= total;
const upOk = uptime() > 0;
const la = loadavg();
const laOk = la.length === 3;
const par = availableParallelism();
const parOk = par > 0;

// --- cpus -------------------------------------------------------------------
const cs = cpus();
const cpusOk = cs.length > 0;
const c0 = cs[0];
const cpu0Ok = c0.model.length > 0 && c0.speed >= 0 &&
    c0.times.user >= 0 && c0.times.idle >= 0 && c0.times.sys >= 0;
const parLeCpus = par <= cs.length;

// --- network interfaces -----------------------------------------------------
const ni = networkInterfaces();
const niKeys = Object.keys(ni);
const niOk = niKeys.length > 0;
let anyAddrOk = false;
for (const k of niKeys) {
    const arr = ni[k];
    if (arr.length > 0) {
        const a = arr[0];
        if ((a.family === "IPv4" || a.family === "IPv6") && a.address.length > 0) {
            anyAddrOk = true;
        }
    }
}

// --- user info --------------------------------------------------------------
const ui = userInfo();
const uiOk = ui.username.length >= 0 && ui.homedir.length >= 0;

// --- priority ---------------------------------------------------------------
const prio = getPriority();
const prioOk = prio >= -20 && prio <= 19;

// --- properties / constants -------------------------------------------------
const eolOk = EOL === "\n" || EOL === "\r\n";
const devNullOk = devNull.length > 0;
const priLow = constants.priority.PRIORITY_LOW;
const priHighest = constants.priority.PRIORITY_HIGHEST;
const priOk = priLow === 19 && priHighest === -20;
const uvOk = constants.libuv.UV_UDP_REUSEADDR === 4;
const sigOk = constants.signals.SIGINT === 2;

describe("node:os full surface", () => {
    test("platform enum", () => expect(platOk).toBe(true));
    test("arch non-empty", () => expect(archOk).toBe(true));
    test("type non-empty", () => expect(typeOk).toBe(true));
    test("endianness LE/BE", () => expect(endOk).toBe(true));
    test("machine non-empty", () => expect(machineOk).toBe(true));
    test("release non-empty", () => expect(releaseOk).toBe(true));
    test("version non-empty", () => expect(versionOk).toBe(true));
    test("hostname non-empty", () => expect(hostOk).toBe(true));
    test("homedir non-empty", () => expect(homeOk).toBe(true));
    test("tmpdir no trailing sep", () => expect(tmpOk).toBe(true));
    test("mem totals sane", () => expect(memOk).toBe(true));
    test("uptime > 0", () => expect(upOk).toBe(true));
    test("loadavg length 3", () => expect(laOk).toBe(true));
    test("availableParallelism > 0", () => expect(parOk).toBe(true));
    test("cpus non-empty", () => expect(cpusOk).toBe(true));
    test("cpu[0] shape", () => expect(cpu0Ok).toBe(true));
    test("availableParallelism <= cpus", () => expect(parLeCpus).toBe(true));
    test("networkInterfaces non-empty", () => expect(niOk).toBe(true));
    test("some interface has an address", () => expect(anyAddrOk).toBe(true));
    test("userInfo shape", () => expect(uiOk).toBe(true));
    test("getPriority in range", () => expect(prioOk).toBe(true));
    test("EOL literal", () => expect(eolOk).toBe(true));
    test("devNull non-empty", () => expect(devNullOk).toBe(true));
    test("constants.priority values", () => expect(priOk).toBe(true));
    test("constants.libuv value", () => expect(uvOk).toBe(true));
    test("constants.signals.SIGINT", () => expect(sigOk).toBe(true));
});
