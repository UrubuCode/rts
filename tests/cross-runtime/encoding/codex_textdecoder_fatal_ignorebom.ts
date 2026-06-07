// Cross-runtime: TextDecoder fatal and ignoreBOM behavior.
const bomText = new Uint8Array([0xef, 0xbb, 0xbf, 65]);
console.log(new TextDecoder("utf-8").decode(bomText));
console.log(new TextDecoder("utf-8", { ignoreBOM: true }).decode(bomText).charCodeAt(0));

try {
  new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array([0xff]));
} catch (e: any) {
  console.log(e.constructor.name);
}
