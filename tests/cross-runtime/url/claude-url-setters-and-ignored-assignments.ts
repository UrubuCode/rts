// Cross-runtime: every URL component is an accessor, and several assignments to
// them are SILENTLY IGNORED rather than throwing. This pins which ones stick,
// which are dropped, and what each setter re-encodes on the way in.

const fresh = function (): URL {
  return new URL("https://user:pass@example.com:8443/p/q?a=1#h");
};

// protocol: only a swap between two special schemes, or two non-special ones.
const proto = fresh();
proto.protocol = "http:";
console.log("proto_special_swap=" + proto.protocol + " port=" + proto.port);
proto.protocol = "ftp:";
console.log("proto_to_ftp=" + proto.protocol);
proto.protocol = "mailto:";
console.log("proto_to_opaque=" + proto.protocol);
proto.protocol = "wss";
console.log("proto_no_colon=" + proto.protocol);
proto.protocol = "";
console.log("proto_empty=" + proto.protocol);
proto.protocol = "1nvalid:";
console.log("proto_invalid=" + proto.protocol);
proto.protocol = "HTTPS:";
console.log("proto_uppercase=" + proto.protocol);

// A default port is dropped the moment the scheme makes it default.
const swap = new URL("http://example.com:443/");
console.log("before_swap=" + swap.href);
swap.protocol = "https:";
console.log("after_swap=" + swap.href + " port=" + JSON.stringify(swap.port));

// port: out of range, non-numeric and empty.
const port = fresh();
port.port = "1234";
console.log("port_set=" + port.port + " host=" + port.host);
port.port = "443";
console.log("port_default=" + JSON.stringify(port.port));
port.port = "65536";
console.log("port_over=" + JSON.stringify(port.port));
port.port = "abc";
console.log("port_text=" + JSON.stringify(port.port));
port.port = "12abc";
console.log("port_prefix=" + JSON.stringify(port.port));
port.port = "-1";
console.log("port_negative=" + JSON.stringify(port.port));
port.port = "";
console.log("port_empty=" + JSON.stringify(port.port));
port.port = 99 as any;
console.log("port_number=" + JSON.stringify(port.port));

// hostname and host: an empty host is refused for a special scheme, and
// hostname ignores an appended port while host accepts one.
const host = fresh();
host.hostname = "other.example";
console.log("hostname_set=" + host.host);
host.hostname = "";
console.log("hostname_empty_ignored=" + host.hostname);
host.hostname = "third.example:99";
console.log("hostname_with_port=" + host.host);
host.host = "fourth.example:1234";
console.log("host_with_port=" + host.host + " port=" + host.port);
host.host = "fifth.example";
console.log("host_without_port_keeps=" + host.host + " port=" + host.port);
host.host = "";
console.log("host_empty_ignored=" + host.host);
host.hostname = "UPPER.example";
console.log("hostname_lowercased=" + host.hostname);
host.hostname = "has space";
console.log("hostname_space_ignored=" + host.hostname);

// pathname: percent-encodes, prefixes a slash, and normalises separators.
const path = fresh();
path.pathname = "a b/c";
console.log("path_encoded=" + path.pathname);
path.pathname = "/x/../y/./z";
console.log("path_dotted=" + path.pathname);
path.pathname = "";
console.log("path_empty=" + JSON.stringify(path.pathname));
path.pathname = "no-slash";
console.log("path_slash_added=" + path.pathname);
path.pathname = "back\\slash";
console.log("path_backslash=" + path.pathname);
path.pathname = "q?x#y";
console.log("path_delims_encoded=" + path.pathname + " search=" + JSON.stringify(path.search) + " hash=" + JSON.stringify(path.hash));
path.pathname = "/é/😀";
console.log("path_unicode=" + path.pathname);

// search and hash: the leading ? and # are optional and never doubled.
const q = fresh();
q.search = "x=1&y=2";
console.log("search_no_mark=" + q.search);
q.search = "?z=3";
console.log("search_with_mark=" + q.search);
q.search = "";
console.log("search_empty=" + JSON.stringify(q.search) + " href=" + q.href);
q.search = "?";
console.log("search_bare_mark=" + JSON.stringify(q.search) + " href=" + q.href);
q.search = "a b&c=d e";
console.log("search_encoded=" + q.search);
q.hash = "frag";
console.log("hash_no_mark=" + q.hash);
q.hash = "#frag2";
console.log("hash_with_mark=" + q.hash);
q.hash = "";
console.log("hash_empty=" + JSON.stringify(q.hash) + " href=" + q.href);
q.hash = "#";
console.log("hash_bare_mark=" + JSON.stringify(q.hash) + " href=" + q.href);
q.hash = "a b\"c";
console.log("hash_encoded=" + q.hash);

// username / password.
const cred = fresh();
cred.username = "new user";
cred.password = "p:ss@word";
console.log("cred=" + cred.username + " | " + cred.password);
cred.username = "";
cred.password = "";
console.log("cred_cleared=" + cred.href);

// href replaces everything, and a bad value throws instead of being ignored.
const href = fresh();
href.href = "http://other.example/z?w=1";
console.log("href_set=" + href.href + " host=" + href.host + " search=" + href.search);
try {
  href.href = "not a url";
  console.log("href_bad=no-throw:" + href.href);
} catch (e: any) {
  console.log("href_bad=" + e.constructor.name + " still=" + href.href);
}

// origin and searchParams are getter-only, so there is no assignment to ignore.
const originDesc = Object.getOwnPropertyDescriptor(URL.prototype, "origin") as any;
const paramsDesc = Object.getOwnPropertyDescriptor(URL.prototype, "searchParams") as any;
console.log("origin_getter=" + typeof originDesc.get + " setter=" + typeof originDesc.set);
console.log("searchParams_getter=" + typeof paramsDesc.get + " setter=" + typeof paramsDesc.set);
const ro = fresh();
console.log("origin=" + ro.origin);
console.log("searchParams_stable=" + (ro.searchParams === ro.searchParams));
console.log("settable=" + ["href", "protocol", "host", "hostname", "port", "pathname", "search", "hash", "username", "password"].filter(function (k) {
  return typeof (Object.getOwnPropertyDescriptor(URL.prototype, k) as any).set === "function";
}).join(","));
