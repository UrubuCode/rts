// GC stress (lean) — heap (Entry/HandleTable) agora no rts-engine. Churn de
// strings + objetos forçando muitos ciclos de GC (>256 alloc/tick); janela viva
// por índice fixo (sem shift O(n) nem for-of). Verifica integridade no fim.
class Node {
  constructor(public id: number, public tag: string) {}
}

function run(): number {
  const W = 256;
  const live: Node[] = [];
  for (let i = 0; i < W; i++) live.push(new Node(i, "seed" + i));
  let checksum = 0;
  for (let i = 0; i < 20000; i++) {
    const t = "node-" + i;            // gc String nova a cada iter
    const n = new Node(i, t);         // gc Instance nova
    const slot = i % W;
    const old = live[slot];           // o antigo vira lixo → coletável
    checksum = (checksum + old.id) % 1000000007;
    live[slot] = n;                   // overwrite (sem shift)
  }
  let liveSum = 0;
  for (let j = 0; j < W; j++) {
    const n = live[j];
    liveSum = (liveSum + n.id + n.tag.length) % 1000000007;  // íntegros?
  }
  return (checksum + liveSum) % 1000000007;
}

const r = run();
console.log("gc_stress checksum=" + r);
console.log("STRESS_OK");
