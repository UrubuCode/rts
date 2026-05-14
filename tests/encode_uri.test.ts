import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const url = "https://example.com/path with spaces";
print("enc=" + encodeURI(url));
print("dec=" + decodeURI(encodeURI(url)));

// reserved chars preservados por encodeURI
print("slash_uri=" + encodeURI("a/b"));
print("amp_uri=" + encodeURI("a&b"));

// encodeURIComponent escapa tudo
print("slash_comp=" + encodeURIComponent("/"));
print("amp_comp=" + encodeURIComponent("&"));

// decodeURI preserva %2F (reserved)
print("dec_reserved=" + decodeURI("a%2Fb"));
// decodeURIComponent decodifica tudo
print("dec_comp=" + decodeURIComponent("a%2Fb"));

describe("encodeURI / decodeURI (#775)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "enc=https://example.com/path%20with%20spaces\n" +
      "dec=https://example.com/path with spaces\n" +
      "slash_uri=a/b\n" +
      "amp_uri=a&b\n" +
      "slash_comp=%2F\n" +
      "amp_comp=%26\n" +
      "dec_reserved=a%2Fb\n" +
      "dec_comp=a/b\n"
    )
  );
});
