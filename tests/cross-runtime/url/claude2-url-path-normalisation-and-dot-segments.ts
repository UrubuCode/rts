// ONE thing: the URL parser's path state machine — dot-segment removal, the
// backslash-as-separator rule for special schemes, an empty path defaulting to
// "/" and the Windows-drive-letter special case for file:.
function show(input: string, base?: string) {
  try {
    const u = base ? new URL(input, base) : new URL(input);
    console.log(JSON.stringify(input) + (base ? " @" + JSON.stringify(base) : "") +
      " -> path=" + JSON.stringify(u.pathname) + " href=" + u.href);
  } catch (e: any) {
    console.log(JSON.stringify(input) + (base ? " @" + JSON.stringify(base) : "") + " -> " + e.constructor.name);
  }
}

// Dot segments are removed at parse time, not lazily.
show("http://h/a/b/../c");
show("http://h/a/b/./c");
show("http://h/a/../../c");
show("http://h/../c");
show("http://h/a/b/..");
show("http://h/a/b/.");
show("http://h/a/b/../");
show("http://h/./././a");
show("http://h/a/%2e%2e/b");
show("http://h/a/%2E/b");
show("http://h/a/...");
show("http://h/a/....");

// Backslashes are separators for a SPECIAL scheme and literal otherwise.
show("http://h/a\\b\\c");
show("http:\\\\h\\a");
show("mailto:a\\b");
show("foo://h/a\\b");

// An empty path on a special scheme becomes "/"; on a non-special one it stays.
show("http://h");
show("http://h?q");
show("http://h#f");
show("foo://h");
show("foo:/a");
show("foo:a");

// Relative resolution against a base exercises the same machine.
show("../x", "http://h/a/b/c");
show("./x", "http://h/a/b/c");
show("/x", "http://h/a/b/c");
show("x", "http://h/a/b/c");
show("", "http://h/a/b/c");
show("?q", "http://h/a/b/c?old#frag");
show("#f", "http://h/a/b/c?old#frag");
show("//other/x", "http://h/a/b/c");
show("https://other/x", "http://h/a/b/c");
show("..", "http://h/a/b/");

// file: gets the drive-letter rule and a host that is normally empty.
show("file:///c:/a/../b");
show("file://localhost/a");
show("file:///a/b/..");

// Percent-encoding in the path uses the path set, and the parser preserves case
// of the hex digits it did not create.
show("http://h/a b");
show("http://h/a%2fb");
show("http://h/a%zz");
show("http://h/\u00e9");
show("http://h/a?b c#d e");

// The path setter runs the same machine, and a leading slash is implied.
const u = new URL("http://h/base/x");
u.pathname = "a/../b";
console.log("setter1=" + u.pathname);
u.pathname = "/c/./d/";
console.log("setter2=" + u.pathname);
u.pathname = "";
console.log("setter3=" + JSON.stringify(u.pathname) + " href=" + u.href);
