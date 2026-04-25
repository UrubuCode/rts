// POC — exercita os 16 namespaces ativos no RTS + capacidades TS.
//
// Simula uma ferramenta de CLI que:
//   1. imprime info de ambiente (os, env, process)
//   2. calcula pi via Machin (math + bigfloat)
//   3. mantem contadores por chave (collections.map)
//   4. escreve e le um arquivo temp com um texto processado
//      (path + fs + string + buffer + crypto + fmt)
//   5. usa try/catch pra lidar com erro de parse (fmt + gc error slot)
//   6. usa ?? / ?. / tail call / first-class fns / compound assign
//      para exercitar o codegen
//
// Executavel via:
//   target\release\rts.exe run examples\poc_toolkit.ts
//   RTS_JIT=1 target\release\rts.exe run examples\poc_toolkit.ts
//   target\release\rts.exe compile -p examples\poc_toolkit.ts out && ./out.exe

import {
  bigfloat,
  buffer,
  collections,
  crypto,
  env,
  fmt,
  fs,
  hash,
  io,
  math,
  os,
  path,
  process,
  string,
  time,
} from "rts";

// ── 1. Info de ambiente ─────────────────────────────────────────────
function section(title: string): void {
  io.print(`\n── ${string.to_upper(title)} ──`);
}

function showEnv(): void {
  section("env");
  io.print(`platform = ${os.platform()}/${os.arch()} (${os.family()})`);
  io.print(`pid = ${process.pid()}`);
  io.print(`cwd = ${env.cwd()}`);
  io.print(`home = ${os.home_dir()}`);

  const rtsJit = env.get_var("RTS_JIT");
  io.print(`RTS_JIT = "${rtsJit}" (vazio significa AOT)`);
}

// ── 2. Pi via Machin (30 digitos) ───────────────────────────────────
//
// Demonstra: bigfloat handles, first-class fn pointer, tail-style loop,
// compound assign em i32, ternario.

const PREC: i32 = 30;

function atanInverseTail(n: i32, result: u64, power: u64, sign: i32, i: i32, terms: i32): u64 {
  if (i >= terms) { return result; }

  const n_big = bigfloat.from_i64(n, PREC);
  const n_sq = bigfloat.mul(n_big, n_big);

  const next_power = bigfloat.div(power, n_sq);
  bigfloat.free(power);

  const denom = bigfloat.from_i64(2 * i + 1, PREC);
  const term = bigfloat.div(next_power, denom);
  bigfloat.free(denom);

  const new_result = sign > 0 ? bigfloat.add(result, term) : bigfloat.sub(result, term);
  bigfloat.free(result);
  bigfloat.free(term);

  bigfloat.free(n_big);
  bigfloat.free(n_sq);

  // Tail call — nao estoura stack mesmo em 80+ iteracoes.
  return atanInverseTail(n, new_result, next_power, -sign, i + 1, terms);
}

function atanInverse(n: i32, terms: i32): u64 {
  const zero = bigfloat.zero(PREC);
  const one = bigfloat.from_i64(1, PREC);
  const n_big = bigfloat.from_i64(n, PREC);
  const power0 = bigfloat.div(one, n_big);
  const result0 = bigfloat.add(zero, power0);
  bigfloat.free(zero);
  bigfloat.free(one);
  bigfloat.free(n_big);
  return atanInverseTail(n, result0, power0, -1, 1, terms);
}

function showPi(): void {
  section("pi via machin");
  const atan_1_5: u64 = atanInverse(5, 80);
  const atan_1_239: u64 = atanInverse(239, 20);
  const sixteen = bigfloat.from_i64(16, PREC);
  const four = bigfloat.from_i64(4, PREC);
  const a = bigfloat.mul(sixteen, atan_1_5);
  const b = bigfloat.mul(four, atan_1_239);
  const pi = bigfloat.sub(a, b);

  io.print(`pi (30 digitos) = ${bigfloat.to_string(pi)}`);
  io.print(`pi (f64)        = ${math.PI}`);
  io.print(`diff f64        = ${bigfloat.to_f64(pi) - math.PI}`);

  bigfloat.free(atan_1_5);
  bigfloat.free(atan_1_239);
  bigfloat.free(sixteen);
  bigfloat.free(four);
  bigfloat.free(a);
  bigfloat.free(b);
  bigfloat.free(pi);
}

// ── 3. Contadores por chave (collections.map) ───────────────────────
function tally(tokens: string, counts: u64): void {
  // Comparacao via code-point (i64) porque string == string ainda
  // compara handles, nao conteudo (issue follow-up).
  const SPACE: i32 = 32;
  const n: i32 = string.char_count(tokens);
  let word = "";
  let wordLen: i32 = 0;
  let i: i32 = 0;
  while (i < n) {
    const code = string.char_code_at(tokens, i);
    if (code == SPACE) {
      if (wordLen > 0) {
        const prev = collections.map_get(counts, word);
        collections.map_set(counts, word, prev + 1);
        word = "";
        wordLen = 0;
      }
    } else {
      word = word + string.char_at(tokens, i);
      wordLen += 1;
    }
    i += 1;
  }
  if (wordLen > 0) {
    const prev = collections.map_get(counts, word);
    collections.map_set(counts, word, prev + 1);
  }
}

function showCounts(): void {
  section("collections");
  const counts = collections.map_new();
  tally("foo bar foo baz bar foo", counts);
  io.print(`foo count = ${collections.map_get(counts, "foo")}`);
  io.print(`bar count = ${collections.map_get(counts, "bar")}`);
  io.print(`baz count = ${collections.map_get(counts, "baz")}`);
  io.print(`unknown ?? 0 = ${collections.map_get(counts, "nope") ?? 0}`);
  io.print(`map len = ${collections.map_len(counts)}`);
  collections.map_free(counts);
}

// ── 4. Fluxo real de arquivo ────────────────────────────────────────
function showFileRoundTrip(): void {
  section("file round trip");

  const tmpRoot = os.temp_dir();
  const fileName = `rts_poc_${process.pid()}.txt`;
  const fullPath = path.join(tmpRoot, fileName);

  const payload = "hello from RTS POC\n";
  fs.write(fullPath, payload);
  io.print(`size on disk: ${fs.size(fullPath)} bytes`);

  // Le bytes brutos para um buffer pre-alocado
  const sizeOnDisk: i32 = fs.size(fullPath);
  const buf = buffer.alloc(sizeOnDisk);
  const n: i32 = fs.read(fullPath, buffer.ptr(buf), sizeOnDisk);
  io.print(`read ${n} bytes`);

  // Hash + base64 direto sobre os bytes do buffer
  io.print(`sha256 = ${crypto.sha256_bytes(buffer.ptr(buf), n)}`);
  io.print(`base64 = ${crypto.base64_encode(buffer.ptr(buf), n)}`);
  io.print(`hex    = ${crypto.hex_encode(buffer.ptr(buf), n)}`);

  // Buffer.to_string converte UTF-8 para string handle
  io.print(`text   = ${buffer.to_string(buf)}`);

  buffer.free(buf);
  fs.remove_file(fullPath);
}

// ── 5. Parse + try/catch ────────────────────────────────────────────
function parseOr(s: string, fallback: i32): i32 {
  const v = fmt.parse_i64(s);
  // i64::MIN e o sentinel de erro do parse_i64
  if (v == -9223372036854775808) {
    throw `invalid int: "${s}"`;
  }
  return v;
}

function showParse(): void {
  section("parse + try/catch");
  // try/catch fase 1 (issue #128): throw nao interrompe fluxo, so seta
  // slot de erro. Por isso testamos o slot manualmente em vez de
  // confiar em catch. Quando #128 for resolvido, voltar ao padrao
  // try/catch idiomatico.
  try {
    const a = parseOr("42", 0);
    io.print(`parsed 42 → ${a}`);
  } catch (e) {
    io.print(`caught: ${e}`);
  } finally {
    io.print(`finally: parse section done`);
  }

  io.print(`hex(255) = ${fmt.fmt_hex(255)}`);
  io.print(`bin(10)  = ${fmt.fmt_bin(10)}`);
  io.print(`pi 4dp   = ${fmt.fmt_f64_prec(math.PI, 4)}`);
}

// ── 6. First-class functions ────────────────────────────────────────
function double(x: i32): i32 { return x * 2; }
function triple(x: i32): i32 { return x * 3; }
function apply(fn: i64, x: i32): i32 { return fn(x); }

function showCallbacks(): void {
  section("first-class fns");
  io.print(`apply(double, 7) = ${apply(double, 7)}`);
  io.print(`apply(triple, 7) = ${apply(triple, 7)}`);
}

// ── 7. Hash / random ────────────────────────────────────────────────
function showCrypto(): void {
  section("crypto + hash");
  const h1 = hash.hash_str("hello");
  const h2 = hash.hash_str("hello");
  io.print(`hash_str deterministico: ${h1 == h2}`);

  const r1 = crypto.random_i64();
  const r2 = crypto.random_i64();
  io.print(`CSPRNG dois valores diferentes: ${r1 != r2}`);

  io.print(`sha256("abc") = ${crypto.sha256_str("abc")}`);
}

// ── 8. Timing ───────────────────────────────────────────────────────
function timed(label: string, fn: i64): void {
  const t0 = time.now_ms();
  fn(0);  // apply-style, arg ignorado
  const elapsed = time.now_ms() - t0;
  io.print(`${label}: ${elapsed} ms`);
}

function heavy(_x: i32): i32 {
  let acc: i64 = 0;
  let j: i32 = 0;
  while (j < 1000000) {
    acc += j;
    j += 1;
  }
  return acc;
}

function sleeper(_x: i32): i32 {
  time.sleep_ms(10);
  return 0;
}

function showTimings(): void {
  section("timings");
  timed("1M add loop", heavy);
  timed("sleep 10 ms ", sleeper);
}

// ── main ────────────────────────────────────────────────────────────
function main(): void {
  io.print("RTS POC — todos os namespaces");
  showEnv();
  showPi();
  showCounts();
  showFileRoundTrip();
  showParse();
  showCallbacks();
  showCrypto();
  showTimings();
  io.print("\n== done ==");
}

main();
