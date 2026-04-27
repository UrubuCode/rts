// http builtin — fetch unificado HTTP/HTTPS sobre net + tls.
//
// Uso:
//   import { fetch } from "http";
//   const r = fetch("https://api.github.com/users/torvalds");
//   if (r.ok()) console.log(r.text());

export { fetch, fetchWith } from "./fetch";
export { Response } from "./response";
export { Request, RequestInit, defaultInit } from "./request";
export { ParsedUrl, parseUrl } from "./url";
