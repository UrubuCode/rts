// Cross-runtime: Headers.getSetCookie.
const h = new Headers();
h.append("set-cookie", "a=1");
h.append("set-cookie", "b=2");
console.log("type=" + typeof h.getSetCookie);
console.log("vals=" + h.getSetCookie().join("|"));
