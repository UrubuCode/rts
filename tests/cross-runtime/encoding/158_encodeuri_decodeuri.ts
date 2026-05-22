// Cross-runtime: encodeURI and decodeURI
const url = "https://example.com/path with spaces";
const encoded = encodeURI(url);
console.log("encoded=" + encoded);

const decoded = decodeURI(encoded);
console.log("decoded=" + decoded);

// encodeURIComponent
const param = "hello world & foo=bar";
const encodedComp = encodeURIComponent(param);
console.log("component=" + encodedComp);

const decodedComp = decodeURIComponent(encodedComp);
console.log("decoded_comp=" + decodedComp);

// Special chars
console.log("slash=" + encodeURIComponent("/"));
console.log("question=" + encodeURIComponent("?"));
console.log("amp=" + encodeURIComponent("&"));
