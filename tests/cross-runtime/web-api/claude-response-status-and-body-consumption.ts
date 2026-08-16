// Cross-runtime: Response validates its status at construction (RangeError
// outside 200-599), a body may be read exactly ONCE, and clone() is the only
// way to read it twice. No network is involved — every object is built locally.

(async function (): Promise<void> {
  // Defaults.
  const plain = new Response();
  console.log("default=" + plain.status + " ok=" + plain.ok + " type=" + plain.type + " statusText=" + JSON.stringify(plain.statusText) + " bodyUsed=" + plain.bodyUsed);
  console.log("default_body_null=" + (plain.body === null));
  console.log("redirected=" + plain.redirected + " url=" + JSON.stringify(plain.url));

  // Status validation and the ok window.
  const statuses: number[] = [199, 200, 204, 299, 300, 399, 400, 599, 600, 1000, 0, -1];
  for (const st of statuses) {
    try {
      const r = new Response(null, { status: st });
      console.log("status[" + st + "]=" + r.status + " ok=" + r.ok);
    } catch (e: any) {
      console.log("status[" + st + "]=" + e.constructor.name);
    }
  }
  console.log("statusText_kept=" + JSON.stringify(new Response(null, { status: 404, statusText: "Nope" }).statusText));

  // Response.error() is a network error: status 0 and never ok.
  const err = Response.error();
  console.log("error=" + err.status + " type=" + err.type + " ok=" + err.ok + " bodyUsed=" + err.bodyUsed);

  // Response.redirect() only accepts the redirect statuses.
  for (const st of [300, 301, 302, 303, 304, 307, 308, 200, 399]) {
    try {
      const r = Response.redirect("https://example.com/x", st);
      console.log("redirect[" + st + "]=" + r.status + " loc=" + r.headers.get("location"));
    } catch (e: any) {
      console.log("redirect[" + st + "]=" + e.constructor.name);
    }
  }
  console.log("redirect_default=" + Response.redirect("https://example.com/x").status);

  // Response.json() serialises and sets a JSON content type.
  const rj = Response.json({ a: [1, 2], b: null });
  console.log("json_status=" + rj.status + " ct_starts=" + (rj.headers.get("content-type") as string).indexOf("application/json"));
  console.log("json_body=" + (await rj.json()).a.join("-"));
  console.log("json_with_status=" + Response.json({}, { status: 418 }).status);
  try {
    Response.json({ self: null } as any);
    console.log("json_ok=yes");
  } catch (e: any) {
    console.log("json_ok=" + e.constructor.name);
  }
  const circular: any = {};
  circular.self = circular;
  try {
    Response.json(circular);
    console.log("json_circular=accepted");
  } catch (e: any) {
    console.log("json_circular=" + e.constructor.name);
  }

  // A body is single-use.
  const once = new Response("hello");
  console.log("used_before=" + once.bodyUsed + " has_body=" + (once.body !== null));
  console.log("read=" + (await once.text()));
  console.log("used_after=" + once.bodyUsed);
  try {
    await once.text();
    console.log("second_read=accepted");
  } catch (e: any) {
    console.log("second_read=" + e.constructor.name);
  }
  try {
    await once.arrayBuffer();
    console.log("other_reader=accepted");
  } catch (e: any) {
    console.log("other_reader=" + e.constructor.name);
  }

  // clone() gives an independent body; cloning after a read is refused.
  const original = new Response("cloneable");
  const copy = original.clone();
  console.log("clone_used=" + original.bodyUsed + "," + copy.bodyUsed);
  console.log("clone_a=" + (await original.text()));
  console.log("clone_b=" + (await copy.text()));

  // The body readers over each source shape.
  console.log("from_string=" + (await new Response("abc").text()));
  console.log("from_bytes=" + (await new Response(new Uint8Array([104, 105])).text()));
  console.log("from_buffer=" + (await new Response(new Uint8Array([104, 105]).buffer).text()));
  console.log("from_blob=" + (await new Response(new Blob(["blobbed"])).text()));
  console.log("from_params=" + (await new Response(new URLSearchParams("a=1&b=2")).text()));
  console.log("from_null=" + JSON.stringify(await new Response(null).text()));
  console.log("as_arrayBuffer=" + (await new Response("hi").arrayBuffer()).byteLength);
  const asBlob = await new Response("hi", { headers: { "content-type": "a/b" } }).blob();
  console.log("as_blob=" + asBlob.size + " type=" + asBlob.type);
  console.log("as_bytes=" + Array.from(await new Response("hi").bytes()).join(","));
  try {
    await new Response("not json").json();
    console.log("bad_json=accepted");
  } catch (e: any) {
    console.log("bad_json=" + e.constructor.name);
  }

  // Headers passed in are normalised and the object is independent.
  const withHeaders = new Response("x", { headers: { "X-One": "1", "x-two": "2" } });
  console.log("headers=" + [...withHeaders.headers.keys()].filter(function (k) { return k.indexOf("x-") === 0; }).join(","));
  withHeaders.headers.set("x-three", "3");
  console.log("headers_mutable=" + withHeaders.headers.get("x-three"));

  // Request mirrors the same body contract.
  const rq = new Request("https://example.com/p", { method: "post", body: "payload", headers: { "X-A": "1" } });
  console.log("request=" + rq.method + " " + rq.url + " used=" + rq.bodyUsed + " hdr=" + rq.headers.get("x-a"));
  const rqCopy = rq.clone();
  console.log("request_read=" + (await rq.text()) + " clone=" + (await rqCopy.text()));
  console.log("request_used=" + rq.bodyUsed);
  console.log("request_from_request=" + new Request(new Request("https://example.com/q", { method: "PUT" })).method);
  try {
    new Request("relative/path");
    console.log("request_relative=accepted");
  } catch (e: any) {
    console.log("request_relative=" + e.constructor.name);
  }
  console.log("tags=" + Object.prototype.toString.call(plain) + Object.prototype.toString.call(rq));
})();
