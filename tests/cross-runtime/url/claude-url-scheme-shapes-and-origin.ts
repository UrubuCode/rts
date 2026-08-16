// Cross-runtime: the URL parser treats a handful of schemes as SPECIAL and
// everything else as opaque. This pins where the host/path split lands for
// file:, mailto:, data:, about:, blob: and an unknown scheme — and which of
// them answers a real origin rather than the string "null".

const show = function (input: string): void {
  try {
    const u = new URL(input);
    console.log(input + " -> href=" + u.href + " proto=" + u.protocol + " host=" + JSON.stringify(u.host) + " path=" + JSON.stringify(u.pathname) + " origin=" + u.origin);
  } catch (e: any) {
    console.log(input + " -> " + e.constructor.name);
  }
};

show("https://example.com/a");
show("http://example.com/a");
show("ws://example.com/a");
show("wss://example.com/a");
show("ftp://example.com/a");
show("file:///c:/dir/file.txt");
show("file://host/share");
show("file:///");
show("mailto:user@example.com");
show("mailto:user@example.com?subject=hi");
show("data:text/plain,hello");
show("data:;base64,aGk=");
show("about:blank");
show("about:srcdoc");
show("blob:https://example.com/1234");
show("blob:null/1234");
show("javascript:void(0)");
show("non-special://host/path");
show("non-special:/path");
show("non-special:path");
show("foo:");
show("urn:isbn:0451450523");
show("tel:+15551234");

// A special scheme requires a host; an opaque one does not.
show("https:///a");
show("https://");
show("http://?q=1");
show("non-special://");

// The parser refuses these outright.
const refused: string[] = ["", "   ", "//example.com/a", "/just/a/path", "example.com", "https://[bad", "https://exa mple.com/", "http://%zz.example/"];
for (const s of refused) {
  try {
    console.log("refused[" + JSON.stringify(s) + "]=" + new URL(s).href);
  } catch (e: any) {
    console.log("refused[" + JSON.stringify(s) + "]=" + e.constructor.name);
  }
}

// A relative reference resolves against a base; an absolute one ignores it.
const base = "https://example.com/a/b/c?x=1#f";
const relatives: string[] = ["d", "./d", "../d", "../../d", "../../../d", "/d", "//other.example/d", "?q=2", "#g", "", "http://elsewhere.example/z"];
for (const r of relatives) {
  console.log("rel[" + JSON.stringify(r) + "]=" + new URL(r, base).href);
}

// A base that is itself opaque cannot take a path-relative reference.
try {
  console.log("opaque_base=" + new URL("d", "mailto:a@b.com").href);
} catch (e: any) {
  console.log("opaque_base=" + e.constructor.name);
}
console.log("opaque_base_fragment=" + new URL("#f", "mailto:a@b.com").href);
console.log("base_as_url_object=" + new URL("d", new URL(base) as any).href);

// canParse mirrors the constructor exactly.
const probes: Array<[string, string | undefined]> = [
  ["https://x.example", undefined],
  ["/rel", undefined],
  ["/rel", "https://x.example"],
  ["http://[bad", undefined],
  ["", "https://x.example"],
  ["", undefined],
  ["d", "mailto:a@b.com"],
];
for (const p of probes) {
  const ok = p[1] === undefined ? URL.canParse(p[0]) : URL.canParse(p[0], p[1]);
  let ctor = "ok";
  try {
    if (p[1] === undefined) new URL(p[0]);
    else new URL(p[0], p[1]);
  } catch (e: any) {
    ctor = "throw";
  }
  console.log("canParse[" + JSON.stringify(p[0]) + "," + String(p[1]) + "]=" + ok + " ctor=" + ctor);
}

// toString / toJSON / valueOf all answer href.
const u = new URL("https://example.com/p?q=1#f");
console.log("toString=" + (u.toString() === u.href) + " toJSON=" + (u.toJSON() === u.href) + " json=" + JSON.stringify({ u: u }));
console.log("tag=" + Object.prototype.toString.call(u));
console.log("href_is_accessor=" + (typeof (Object.getOwnPropertyDescriptor(URL.prototype, "href") as any).get));
