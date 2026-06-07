// Cross-runtime: padrão produtor/consumidor via funções aninhadas — caso de
// CONTROLE do bug #1357 que PASSA no RTS (sibling-call de fn aninhada funciona
// nesta forma). Mantido como guarda de regressão: documenta a fronteira do bug
// (as variantes 1357_nested_fn_same_name / _sibling_call ainda divergem) e
// garante que este padrão siga verde. Esperado: filled=20 final_queued=20 rounds=4.
function engine(): void {
  const capacity = 20;
  let queued = 0;
  function freeSpace(): number {
    return capacity - queued;
  }
  function fill(n: number): void {
    queued = queued + n;
  }
  function pump(): void {
    let filled = 0;
    let rounds = 0;
    const block = 5;
    while (freeSpace() > 0 && rounds < 100) {
      let n = freeSpace();
      if (n > block) n = block;
      fill(n);
      filled = filled + n;
      rounds = rounds + 1;
    }
    console.log("filled=" + filled + " final_queued=" + queued + " rounds=" + rounds);
  }
  pump();
}
engine();
