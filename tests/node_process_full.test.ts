// node:process — flat synchronous surface.
import { describe, test, expect } from "rts:test";
import {
    cwd,
    platform,
    arch,
    pid,
    uptime,
    hrtime,
    version,
    versions,
    argv,
    argv0,
    execPath,
    title,
    getActiveResourcesInfo,
    env,
    memoryUsage,
    cpuUsage,
} from "node:process";

const mu = memoryUsage();
const muOk = mu.rss > 0 && typeof mu.heapTotal === "number" && typeof mu.external === "number";

const cu = cpuUsage();
const cuOk = cu.user >= 0 && cu.system >= 0;

const theCwd = cwd();
const theCwdOk = theCwd.length > 0;

const plat = platform();
const platOk = plat === "win32" || plat === "linux" || plat === "darwin";

const a = arch();
const archOk = a === "x64" || a === "arm64" || a === "ia32";

const thePid = pid();
const pidOk = thePid > 0;

const up = uptime();
const upOk = up >= 0;

const hr = hrtime();
const hrOk = hr.length === 2 && hr[0] >= 0 && hr[1] >= 0;

const hrDiff = hrtime(hr);
const hrDiffOk = hrDiff.length === 2;

const v = version();
const vOk = v.indexOf("v") === 0;

const vs = versions();
const vsOk = typeof vs.node === "string";

const av = argv();
const avOk = av.length >= 1;

const av0 = argv0();
const av0Ok = av0.length > 0;

const ep = execPath();
const epOk = ep.length > 0;

const t = title();
const tOk = t.length > 0;

const res = getActiveResourcesInfo();
const resOk = res.length === 0;

const e = env();
const eOk = typeof e === "object";

describe("node:process", () => {
    test("cwd", () => expect(theCwdOk).toBe(true));
    test("platform", () => expect(platOk).toBe(true));
    test("arch", () => expect(archOk).toBe(true));
    test("pid", () => expect(pidOk).toBe(true));
    test("uptime", () => expect(upOk).toBe(true));
    test("hrtime array", () => expect(hrOk).toBe(true));
    test("hrtime diff", () => expect(hrDiffOk).toBe(true));
    test("version", () => expect(vOk).toBe(true));
    test("versions.node", () => expect(vsOk).toBe(true));
    test("argv", () => expect(avOk).toBe(true));
    test("argv0", () => expect(av0Ok).toBe(true));
    test("execPath", () => expect(epOk).toBe(true));
    test("title", () => expect(tOk).toBe(true));
    test("getActiveResourcesInfo empty", () => expect(resOk).toBe(true));
    test("env object", () => expect(eOk).toBe(true));
    test("memoryUsage rss", () => expect(muOk).toBe(true));
    test("cpuUsage", () => expect(cuOk).toBe(true));
});
