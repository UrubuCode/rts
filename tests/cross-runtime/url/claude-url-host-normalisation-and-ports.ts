// Cross-runtime: what the parser does to the AUTHORITY of a special scheme —
// the default port is dropped, an IPv4 host is re-serialised from any base, an
// IPv6 host is compressed inside brackets, and tabs/newlines are stripped from
// the input before anything else happens.

const authority = function (input: string): void {
  try {
    const u = new URL(input);
    console.log(JSON.stringify(input) + " -> host=" + u.host + " hostname=" + u.hostname + " port=" + JSON.stringify(u.port) + " href=" + u.href);
  } catch (e: any) {
    console.log(JSON.stringify(input) + " -> " + e.constructor.name);
  }
};

// The default port for each special scheme disappears from href.
authority("https://example.com:443/a");
authority("http://example.com:80/a");
authority("ws://example.com:80/a");
authority("wss://example.com:443/a");
authority("ftp://example.com:21/a");
authority("https://example.com:80/a");
authority("http://example.com:443/a");
authority("http://example.com:8080/a");
authority("http://example.com:0/a");
authority("http://example.com:65535/a");
authority("http://example.com:65536/a");
authority("http://example.com:/a");
authority("non-special://example.com:80/a");

// Case folding and trailing dots in the host.
authority("https://EXAMPLE.COM/A");
authority("https://ExAmPlE.cOm./a");
authority("HTTPS://example.com/a");

// IPv4 is re-serialised in decimal dotted-quad form from any legal spelling.
authority("https://192.168.000.001/");
authority("https://0x7f.1/");
authority("https://0177.0.0.1/");
authority("https://2130706433/");
authority("https://1.1/");
authority("https://999.1.1.1/");
authority("https://1.2.3.4.5/");

// IPv6 is compressed and lower-cased inside its brackets.
authority("https://[2001:DB8:0:0:0:0:0:1]:8080/p");
authority("https://[::1]/");
authority("https://[::ffff:1.2.3.4]/");
authority("https://[0:0:0:0:0:0:0:0]/");
authority("https://[::1]:443/");
authority("https://[:::1]/");
authority("https://[::1/");

// Percent-encoding and IDNA in the host.
authority("https://%65xample.com/");
authority("https://exa%20mple.com/");
authority("https://xn--bcher-kva.example/");

// Backslashes in a special scheme are treated as slashes.
authority("https:\\\\example.com\\a\\b");
authority("https://example.com\\a");
authority("non-special:\\\\example.com\\a");

// Tabs, newlines and carriage returns are removed anywhere in the input.
authority("ht\ttps://exa\nmple.com/a\rb");
authority("https://example.com/a\tb?c\nd#e\rf");
console.log("strip_in_search=" + new URL("https://example.com/?a=1\t2").search);

// Leading and trailing C0 controls and spaces are trimmed.
console.log("trim_spaces=" + JSON.stringify(new URL("   https://example.com/x   ").href));
console.log("trim_controls=" + JSON.stringify(new URL("\u0001https://example.com/x\u0001").href));
console.log("inner_space=" + (function (): string {
  try {
    return new URL("https://example.com/a b").href;
  } catch (e: any) {
    return e.constructor.name;
  }
})());

// Userinfo is kept and percent-encoded, and it never reaches host or origin.
const withUser = new URL("https://us er:p@ss@example.com:8443/p");
console.log("user=" + withUser.username + " pass=" + withUser.password + " host=" + withUser.host + " origin=" + withUser.origin);
console.log("user_href=" + withUser.href);
console.log("empty_user=" + new URL("https://@example.com/").href);
console.log("only_colon=" + new URL("https://:@example.com/").href);

// The port survives a host that already carries one, and host wins over port.
const p = new URL("https://example.com:1234/");
console.log("port_from_host=" + p.port + " host=" + p.host + " hostname=" + p.hostname);
console.log("port_number_type=" + typeof p.port);

// A file: URL has no port and no origin, and Windows drive letters are kept.
authority("file://localhost/c:/x");
authority("file:///C|/x");
authority("file:///./x");
