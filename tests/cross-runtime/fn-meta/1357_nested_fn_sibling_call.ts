// Cross-runtime: função aninhada que CHAMA outra função aninhada irmã.
// Bug RTS #1357 — a forma exata do bug de áudio: dentro de `outer`, a fn `tick`
// chama as siblings `availableFrames`/`consume`. O hoisting não registra siblings
// no escopo umas das outras: "call to undeclared user function availableFrames".
// Bun/Node resolvem normalmente. Esperado: drained=8 remaining=0 iters=1.
function outer(): void {
  let available = 8;
  function availableFrames(): number {
    return available;
  }
  function consume(n: number): void {
    available = available - n;
  }
  function tick(): void {
    let drained = 0;
    let guard = 0;
    while (availableFrames() > 0 && guard < 50) {
      const got = availableFrames();
      consume(got);
      drained = drained + got;
      guard = guard + 1;
    }
    console.log("drained=" + drained + " remaining=" + availableFrames() + " iters=" + guard);
  }
  tick();
}
outer();
