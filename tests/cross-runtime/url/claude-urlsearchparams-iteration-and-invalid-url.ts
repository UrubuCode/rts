// URLSearchParams is iterable (spread / for-of / Array.from / next()), an
// unparseable `new URL` throws ERR_INVALID_URL, and origin follows the scheme.
const p = new URLSearchParams('a=b+x&c=%20d&a=e');
console.log([...p]);
for (const [k, v] of p) console.log(k, v);
console.log(Array.from(p), Array.from(p.keys()), Array.from(p.values()));
console.log([...p.entries()], [...p.keys()], [...p.values()]);
const it = p.entries();
console.log(it.next(), it.next().value, it.next().done, it.next().done);
p.forEach((v, k) => console.log(k, v));
console.log(Object.fromEntries(new URLSearchParams('x=1&y=2')));
console.log(new URLSearchParams([...p]).toString());

for (const bad of ['nope', '', '//x', 'http://']) {
  try { new URL(bad); console.log('nothrow', bad); } catch (e: any) { console.log(String(e), e.code, e.name, e.input, e instanceof TypeError); }
}
try { new URL('/x', 'nope'); console.log('nothrow'); } catch (e: any) { console.log(e.code); }
console.log(new URL('/a?b=1#c', 'https://ex.com:8080/z/y').href);
console.log(new URL('HTTP://EX.com/x').href, new URL('file:///tmp/a').pathname);
console.log(URL.canParse('nope'), URL.parse('nope'));

for (const u of ['foo://host/..', 'blob:https://a.com/u', 'blob:null/x', 'https://a.com:8443/x', 'http://a.com:80/', 'file:///x', 'data:text/plain,hi', 'ws://h/p'])
  console.log(u, new URL(u).origin);
