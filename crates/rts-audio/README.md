# rts-audio — `rts:audio`

A saída de som, o kernel de mixagem e o decodificador OGG/Vorbis do motor novo.

## Regras

1. **Só o que um script não faz.** Dispositivo, anel, `mix_add`, `mix_level`, OGG.
   Voz, grupo, ouvinte, WAV e cache de clipes são do programa.
2. **Nenhuma thread deste crate toca o motor.** A thread do dispositivo (cpal ou
   nula) vê o `Compartilhado` e mais nada — nem `Context`, nem célula.
3. **Um nativo pega os ponteiros num empréstimo curto e trabalha fora dele**, como
   `rts-physics/src/surface.rs`: pânico num quadro `extern "C"` aborta o processo.
4. **Views sobrepostas são recusadas** (`0`), nunca lidas: `dst` e `src` sobre o
   mesmo `ArrayBuffer` é um programa legal e uma leitura aliasada aqui.
5. **O dispositivo nulo é pedido pelo chamador** (`flags` bit 0), nunca escolhido
   por falha: um jogo sem placa de som recebe `0` e fica mudo, como antes.
