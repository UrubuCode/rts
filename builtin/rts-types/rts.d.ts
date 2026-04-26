declare module "rts" {
  export type i8 = number;
  export type u8 = number;
  export type i16 = number;
  export type u16 = number;
  export type i32 = number;
  export type u32 = number;
  export type i64 = number;
  export type u64 = number;
  export type isize = number;
  export type usize = number;
  export type f32 = number;
  export type f64 = number;
  export type bool = boolean;
  export type str = string;

  /**
   * Runtime-managed handle table and string pool.
   */
  export namespace gc {
    /**
     * Converts an i64 to its decimal string and returns a handle.
     */
    export function string_from_i64(value: number): number;
    /**
     * Converts an f64 to its decimal string and returns a handle.
     */
    export function string_from_f64(value: number): number;
    /**
     * Concatenates two string handles and returns a new handle.
     */
    export function string_concat(a: number, b: number): number;
    /**
     * Compara dois string handles por conteudo (memcmp). 1 se iguais, 0 caso contrario.
     */
    export function string_eq(a: number, b: number): number;
    /**
     * Promotes a static (ptr, len) string to a GC handle.
     */
    export function string_from_static(data: string): number;
    /**
     * Allocates a string handle from a (ptr, len) pair. Returns 0 on error.
     */
    export function string_new(data: string): number;
    /**
     * Returns the byte length of the string, or -1 on invalid handle.
     */
    export function string_len(handle: number): number;
    /**
     * Returns the raw pointer to the string buffer, or 0 on invalid handle.
     */
    export function string_ptr(handle: number): number;
    /**
     * Frees the string handle. Returns 1 on success, 0 if already invalid.
     */
    export function string_free(handle: number): number;
  }

  /**
   * Standard input/output primitives backed by std::io.
   */
  export namespace io {
    /**
     * Writes a UTF-8 message followed by newline to stdout.
     */
    export function print(message: string): void;
    /**
     * Writes a UTF-8 message followed by newline to stderr.
     */
    export function eprint(message: string): void;
    /**
     * Writes raw bytes to stdout, returns bytes written or -1 on error.
     */
    export function stdout_write(data: string): number;
    /**
     * Flushes stdout buffer. Returns 0 on success, -1 on error.
     */
    export function stdout_flush(): number;
    /**
     * Writes raw bytes to stderr, returns bytes written or -1 on error.
     */
    export function stderr_write(data: string): number;
    /**
     * Flushes stderr buffer. Returns 0 on success, -1 on error.
     */
    export function stderr_flush(): number;
    /**
     * Reads up to `len` bytes from stdin into buffer. Returns byte count or -1.
     */
    export function stdin_read(bufPtr: number, bufLen: number): number;
    /**
     * Reads a single line from stdin (no terminator) into buffer.
     */
    export function stdin_read_line(bufPtr: number, bufLen: number): number;
  }

  /**
   * Filesystem operations backed by std::fs.
   */
  export namespace fs {
    /**
     * Reads up to `bufLen` bytes from `path` into buffer. Returns byte count or -1.
     */
    export function read(path: string, bufPtr: number, bufLen: number): number;
    /**
     * Reads entire file into buffer, truncating if needed. Returns bytes written or -1.
     */
    export function read_all(path: string, bufPtr: number, bufLen: number): number;
    /**
     * Writes data to `path`, truncating existing contents. Returns bytes written or -1.
     */
    export function write(path: string, data: string): number;
    /**
     * Appends data to `path`, creating it when missing. Returns bytes written or -1.
     */
    export function append(path: string, data: string): number;
    /**
     * Returns 1 if the path exists, 0 otherwise.
     */
    export function exists(path: string): number;
    /**
     * Returns 1 if `path` is a regular file.
     */
    export function is_file(path: string): number;
    /**
     * Returns 1 if `path` is a directory.
     */
    export function is_dir(path: string): number;
    /**
     * Returns file size in bytes, or -1 on error.
     */
    export function size(path: string): number;
    /**
     * Returns last-modified milliseconds since UNIX epoch, or -1 on error.
     */
    export function modified_ms(path: string): number;
    /**
     * Creates the directory at `path`. Returns 0 on success, -1 on error.
     */
    export function create_dir(path: string): number;
    /**
     * Creates the directory and any missing parents.
     */
    export function create_dir_all(path: string): number;
    /**
     * Removes an empty directory.
     */
    export function remove_dir(path: string): number;
    /**
     * Removes a directory and all of its contents.
     */
    export function remove_dir_all(path: string): number;
    /**
     * Removes a file.
     */
    export function remove_file(path: string): number;
    /**
     * Renames `from` to `to`. Returns 0 on success, -1 on error.
     */
    export function rename(from: string, to: string): number;
    /**
     * Copies file contents. Returns bytes copied or -1.
     */
    export function copy(from: string, to: string): number;
  }

  /**
   * Floating-point / integer intrinsics and a seeded xorshift PRNG.
   */
  export namespace math {
    /**
     * Largest integer <= x.
     */
    export function floor(x: number): number;
    /**
     * Smallest integer >= x.
     */
    export function ceil(x: number): number;
    /**
     * Rounds to nearest; ties go to +Infinity to match JS semantics.
     */
    export function round(x: number): number;
    /**
     * Truncates fractional part (rounds toward zero).
     */
    export function trunc(x: number): number;
    /**
     * Square root.
     */
    export function sqrt(x: number): number;
    /**
     * Cube root.
     */
    export function cbrt(x: number): number;
    /**
     * base raised to exp.
     */
    export function pow(base: number, exp: number): number;
    /**
     * e^x.
     */
    export function exp(x: number): number;
    /**
     * Natural logarithm (base e).
     */
    export function ln(x: number): number;
    /**
     * Base-2 logarithm.
     */
    export function log2(x: number): number;
    /**
     * Base-10 logarithm.
     */
    export function log10(x: number): number;
    /**
     * Absolute value (f64).
     */
    export function abs_f64(x: number): number;
    /**
     * Absolute value (i64); i64::MIN maps to itself (wrapping).
     */
    export function abs_i64(x: number): number;
    /**
     * Sine (radians).
     */
    export function sin(x: number): number;
    /**
     * Cosine (radians).
     */
    export function cos(x: number): number;
    /**
     * Tangent (radians).
     */
    export function tan(x: number): number;
    /**
     * Arc sine (returns radians).
     */
    export function asin(x: number): number;
    /**
     * Arc cosine (returns radians).
     */
    export function acos(x: number): number;
    /**
     * Arc tangent (returns radians).
     */
    export function atan(x: number): number;
    /**
     * atan2(y, x) — angle (radians) of the 2D vector (x, y).
     */
    export function atan2(y: number, x: number): number;
    /**
     * Minimum of two f64 values (NaN-aware).
     */
    export function min_f64(a: number, b: number): number;
    /**
     * Maximum of two f64 values (NaN-aware).
     */
    export function max_f64(a: number, b: number): number;
    /**
     * Minimum of two i64 values.
     */
    export function min_i64(a: number, b: number): number;
    /**
     * Maximum of two i64 values.
     */
    export function max_i64(a: number, b: number): number;
    /**
     * Clamps x into [lo, hi]. NaN propagates.
     */
    export function clamp_f64(x: number, lo: number, hi: number): number;
    /**
     * Clamps x into [lo, hi].
     */
    export function clamp_i64(x: number, lo: number, hi: number): number;
    /**
     * Uniform f64 in [0, 1) from a thread-local xorshift64 PRNG.
     */
    export function random_f64(): number;
    /**
     * Uniform i64 in [lo, hi). Returns lo when lo >= hi.
     */
    export function random_i64_range(lo: number, hi: number): number;
    /**
     * Seeds the PRNG. Zero is replaced by the default seed.
     */
    export function seed(s: number): void;
    /**
     * Archimedes' constant.
     */
    export const readonly PI: number;
    /**
     * Euler's number.
     */
    export const readonly E: number;
    /**
     * Positive infinity.
     */
    export const readonly INFINITY: number;
    /**
     * Quiet NaN.
     */
    export const readonly NAN: number;
  }

  /**
   * Aritmetica com overflow explicito (checked/saturating/wrapping) e bit ops.
   */
  export namespace num {
    /**
     * a + b com overflow; retorna i64::MIN como sentinela em overflow.
     */
    export function checked_add(a: number, b: number): number;
    /**
     * a - b com overflow; retorna i64::MIN como sentinela em overflow.
     */
    export function checked_sub(a: number, b: number): number;
    /**
     * a * b com overflow; retorna i64::MIN como sentinela em overflow.
     */
    export function checked_mul(a: number, b: number): number;
    /**
     * a / b; retorna i64::MIN se b == 0 ou overflow (i64::MIN / -1).
     */
    export function checked_div(a: number, b: number): number;
    /**
     * a + b com saturation em i64::MIN/MAX.
     */
    export function saturating_add(a: number, b: number): number;
    /**
     * a - b com saturation em i64::MIN/MAX.
     */
    export function saturating_sub(a: number, b: number): number;
    /**
     * a * b com saturation em i64::MIN/MAX.
     */
    export function saturating_mul(a: number, b: number): number;
    /**
     * a + b modulo 2^64.
     */
    export function wrapping_add(a: number, b: number): number;
    /**
     * a - b modulo 2^64.
     */
    export function wrapping_sub(a: number, b: number): number;
    /**
     * a * b modulo 2^64.
     */
    export function wrapping_mul(a: number, b: number): number;
    /**
     * -a modulo 2^64 (i64::MIN.wrapping_neg() == i64::MIN).
     */
    export function wrapping_neg(a: number): number;
    /**
     * a << (n & 63) — shift count masked.
     */
    export function wrapping_shl(a: number, n: number): number;
    /**
     * a >> (n & 63) (arithmetic shift).
     */
    export function wrapping_shr(a: number, n: number): number;
    /**
     * Numero de bits 1 em a.
     */
    export function count_ones(a: number): number;
    /**
     * Numero de bits 0 em a.
     */
    export function count_zeros(a: number): number;
    /**
     * Numero de zeros leading em a.
     */
    export function leading_zeros(a: number): number;
    /**
     * Numero de zeros trailing em a.
     */
    export function trailing_zeros(a: number): number;
    /**
     * a rotacionado n bits para a esquerda.
     */
    export function rotate_left(a: number, n: number): number;
    /**
     * a rotacionado n bits para a direita.
     */
    export function rotate_right(a: number, n: number): number;
    /**
     * Bits invertidos (LSB->MSB).
     */
    export function reverse_bits(a: number): number;
    /**
     * Bytes invertidos (endianness flip).
     */
    export function swap_bytes(a: number): number;
  }

  /**
   * std::mem: layout (size_of/align_of), swap, drop, forget.
   */
  export namespace mem {
    /**
     * Tamanho em bytes de um i64 (= 8).
     */
    export const size_of_i64: number;
    /**
     * Tamanho em bytes de um f64 (= 8).
     */
    export const size_of_f64: number;
    /**
     * Tamanho em bytes de um i32 (= 4).
     */
    export const size_of_i32: number;
    /**
     * Tamanho em bytes de um bool (= 1).
     */
    export const size_of_bool: number;
    /**
     * Alinhamento de i64 (= 8).
     */
    export const align_of_i64: number;
    /**
     * Alinhamento de f64 (= 8).
     */
    export const align_of_f64: number;
    /**
     * Retorna `b` (use idiom: `let old = mem.swap_i64(a, b)`).
     */
    export function swap_i64(a: number, b: number): number;
    /**
     * Forca free de um handle GC. Equivalente a gc.string_free / etc.
     */
    export function drop_handle(h: number): void;
    /**
     * Esquece handle sem rodar drop — vaza memoria intencionalmente.
     */
    export function forget_handle(h: number): void;
    /**
     * Idiom: `mem.replace_i64(slot, new)` — retorna slot e usa caller pra escrever.
     */
    export function replace_i64(slot: number, new_val: number): number;
  }

  /**
   * Captura de stack traces via std::backtrace::Backtrace.
   */
  export namespace backtrace {
    /**
     * Captura backtrace do call stack atual. Retorna handle.
     */
    export function capture(): number;
    /**
     * Captura backtrace se RUST_BACKTRACE estiver set; retorna 0 caso contrario.
     */
    export function capture_if_enabled(): number;
    /**
     * True se RUST_BACKTRACE=1 (ou full) esta no env.
     */
    export function is_enabled(): boolean;
    /**
     * Formata o backtrace em string. Retorna handle de string GC.
     */
    export function to_string(handle: number): number;
    /**
     * Libera a backtrace.
     */
    export function free(handle: number): void;
  }

  /**
   * Allocator raw via std::alloc. UNSAFE — pareie alloc/dealloc com mesmo size/align.
   */
  export namespace alloc {
    /**
     * Aloca size bytes alinhados a `align`. Retorna ponteiro ou 0 em falha.
     */
    export function alloc(size: number, align: number): number;
    /**
     * Aloca size bytes zerados, alinhados a `align`.
     */
    export function alloc_zeroed(size: number, align: number): number;
    /**
     * Libera ptr previamente alocado com mesmo size/align.
     */
    export function dealloc(ptr: number, size: number, align: number): void;
    /**
     * Realoca ptr (size_old, align) para new_size. Retorna novo ptr ou 0.
     */
    export function realloc(ptr: number, size_old: number, align: number, new_size: number): number;
  }

  /**
   * Arbitrary-precision decimal floating-point via handle table.
   */
  export namespace bigfloat {
    /**
     * Allocates a zero big float with the given decimal precision.
     */
    export function zero(precision: number): number;
    /**
     * Converts an f64 into a big float rounded to `precision` digits.
     */
    export function from_f64(x: number, precision: number): number;
    /**
     * Parses a decimal string into a big float with `precision` digits.
     */
    export function from_str(s: string, precision: number): number;
    /**
     * Creates a big float from an integer with `precision` digits.
     */
    export function from_i64(x: number, precision: number): number;
    /**
     * Lossy conversion back to f64.
     */
    export function to_f64(h: number): number;
    /**
     * Renders as a decimal string at full precision.
     */
    export function to_string(h: number): string;
    /**
     * a + b.
     */
    export function add(a: number, b: number): number;
    /**
     * a - b.
     */
    export function sub(a: number, b: number): number;
    /**
     * a * b.
     */
    export function mul(a: number, b: number): number;
    /**
     * a / b.
     */
    export function div(a: number, b: number): number;
    /**
     * -a.
     */
    export function neg(a: number): number;
    /**
     * Square root. Returns 0 for negative input.
     */
    export function sqrt(a: number): number;
    /**
     * Releases the handle.
     */
    export function free(h: number): void;
  }

  /**
   * Monotonic and wall-clock timestamps, plus blocking sleeps.
   */
  export namespace time {
    /**
     * Monotonic milliseconds since process start.
     */
    export function now_ms(): number;
    /**
     * Monotonic nanoseconds since process start.
     */
    export function now_ns(): number;
    /**
     * Wall-clock milliseconds since the UNIX epoch.
     */
    export function unix_ms(): number;
    /**
     * Wall-clock nanoseconds since the UNIX epoch.
     */
    export function unix_ns(): number;
    /**
     * Sleeps the current thread for `ms` milliseconds.
     */
    export function sleep_ms(ms: number): void;
    /**
     * Sleeps the current thread for `ns` nanoseconds.
     */
    export function sleep_ns(ns: number): void;
  }

  /**
   * Environment variables, process argv, and current working directory.
   */
  export namespace env {
    /**
     * Returns a string handle with the environment variable's value, or 0 when absent.
     */
    export function get_var(name: string): string;
    /**
     * Sets an environment variable.
     */
    export function set_var(name: string, value: string): void;
    /**
     * Removes an environment variable.
     */
    export function remove_var(name: string): void;
    /**
     * Number of command-line arguments (including argv[0]).
     */
    export function args_count(): number;
    /**
     * Returns the argv entry at `index` as a string handle; 0 when out of range.
     */
    export function arg_at(index: number): string;
    /**
     * Returns the current working directory as a string handle.
     */
    export function cwd(): string;
    /**
     * Changes the current working directory. Returns 0 on success, -1 on error.
     */
    export function set_cwd(path: string): number;
  }

  /**
   * Pure path manipulation — no filesystem calls.
   */
  export namespace path {
    /**
     * Joins a base path with a relative fragment.
     */
    export function join(base: string, part: string): string;
    /**
     * Parent directory; 0 when path has no parent (e.g. root or bare filename).
     */
    export function parent(path: string): string;
    /**
     * Final component of the path (file name with extension).
     */
    export function file_name(path: string): string;
    /**
     * File name without extension.
     */
    export function stem(path: string): string;
    /**
     * File extension without leading dot; 0 when absent.
     */
    export function ext(path: string): string;
    /**
     * True when path is absolute for the current target.
     */
    export function is_absolute(path: string): boolean;
    /**
     * Removes `.` and collapses `..` without touching the filesystem.
     */
    export function normalize(path: string): string;
    /**
     * Returns the path with the extension replaced (or added).
     */
    export function with_ext(path: string, ext: string): string;
  }

  /**
   * Binary buffers backed by Vec<u8> in the handle table.
   */
  export namespace buffer {
    /**
     * Allocates a zero-initialised byte buffer of `size` bytes.
     */
    export function alloc(size: number): number;
    /**
     * Alias for alloc — Rust Vec::new already zeroes.
     */
    export function alloc_zeroed(size: number): number;
    /**
     * Releases the buffer handle. Subsequent reads/writes are no-ops.
     */
    export function free(handle: number): void;
    /**
     * Buffer length in bytes, or -1 if the handle is invalid.
     */
    export function len(handle: number): number;
    /**
     * Raw pointer to the buffer start, or 0 when invalid. Unsafe — callers must not outlive the handle.
     */
    export function ptr(handle: number): number;
    /**
     * Reads the byte at `offset`. Returns 0 out of bounds.
     */
    export function read_u8(handle: number, offset: number): number;
    /**
     * Reads a little-endian i32 at `offset`.
     */
    export function read_i32(handle: number, offset: number): number;
    /**
     * Reads a little-endian i64 at `offset`.
     */
    export function read_i64(handle: number, offset: number): number;
    /**
     * Reads a little-endian f64 at `offset`. NaN out of bounds.
     */
    export function read_f64(handle: number, offset: number): number;
    /**
     * Writes `val` as u8 at `offset`. No-op out of bounds.
     */
    export function write_u8(handle: number, offset: number, val: number): void;
    /**
     * Writes a little-endian i32 at `offset`.
     */
    export function write_i32(handle: number, offset: number, val: number): void;
    /**
     * Writes a little-endian i64 at `offset`.
     */
    export function write_i64(handle: number, offset: number, val: number): void;
    /**
     * Writes a little-endian f64 at `offset`.
     */
    export function write_f64(handle: number, offset: number, val: number): void;
    /**
     * Copies `len` bytes from src+srcOff to dst+dstOff. Safe with overlapping src/dst.
     */
    export function copy(dst: number, dstOff: number, src: number, srcOff: number, len: number): void;
    /**
     * Fills the first `len` bytes with `byte`.
     */
    export function fill(handle: number, byte: number, len: number): void;
    /**
     * Interprets buffer contents as UTF-8 and returns a string handle.
     */
    export function to_string(handle: number): string;
  }

  /**
   * Rich string operations beyond the basic gc pool.
   */
  export namespace string {
    /**
     * True when `haystack` contains `needle`.
     */
    export function contains(haystack: string, needle: string): boolean;
    /**
     * True when `s` starts with `prefix`.
     */
    export function starts_with(s: string, prefix: string): boolean;
    /**
     * True when `s` ends with `suffix`.
     */
    export function ends_with(s: string, suffix: string): boolean;
    /**
     * Byte index of first occurrence of `needle`, or -1 when absent.
     */
    export function find(s: string, needle: string): number;
    /**
     * Uppercase copy (Unicode-aware).
     */
    export function to_upper(s: string): string;
    /**
     * Lowercase copy (Unicode-aware).
     */
    export function to_lower(s: string): string;
    /**
     * Removes ASCII + Unicode whitespace from both ends.
     */
    export function trim(s: string): string;
    /**
     * Removes whitespace from the start.
     */
    export function trim_start(s: string): string;
    /**
     * Removes whitespace from the end.
     */
    export function trim_end(s: string): string;
    /**
     * Concatenates `s` with itself `n` times.
     */
    export function repeat(s: string, n: number): string;
    /**
     * Replaces every occurrence of `from` with `to`.
     */
    export function replace(s: string, from: string, to: string): string;
    /**
     * Replaces the first `n` occurrences of `from` with `to`.
     */
    export function replacen(s: string, from: string, to: string, n: number): string;
    /**
     * Unicode codepoint count (chars).
     */
    export function char_count(s: string): number;
    /**
     * Length in UTF-8 bytes.
     */
    export function byte_len(s: string): number;
    /**
     * Character at Unicode index `idx` as a single-char string handle, or 0 out of range.
     */
    export function char_at(s: string, idx: number): string;
    /**
     * Unicode code point at `idx`, or -1 out of range.
     */
    export function char_code_at(s: string, idx: number): number;
  }

  /**
   * Process control: exit/abort, pid, spawn/wait/kill children.
   */
  export namespace process {
    /**
     * Termina o processo corrente com o exit code dado.
     */
    export function exit(code: number): void;
    /**
     * Aborta o processo imediatamente (sem unwind).
     */
    export function abort(): void;
    /**
     * PID do processo corrente.
     */
    export function pid(): number;
    /**
     * Number of command-line arguments (inclui argv[0]).
     */
    export function args_count(): number;
    /**
     * Argumento em `index` como string handle; 0 fora do range.
     */
    export function arg_at(index: number): string;
    /**
     * Dispara `cmd` com argumentos separados por \n. Retorna handle do filho, ou 0 em falha.
     */
    export function spawn(cmd: string, args_newline_separated: string): number;
    /**
     * Aguarda o filho terminar e retorna o exit code. Consome o handle.
     */
    export function wait(child: number): number;
    /**
     * Mata o processo filho. 0 em sucesso, -1 em erro.
     */
    export function kill(child: number): number;
  }

  /**
   * Operacoes raw sobre ponteiros (std::ptr). UNSAFE — caller verifica validez.
   */
  export namespace ptr {
    /**
     * Retorna ponteiro nulo (0).
     */
    export function null(): number;
    /**
     * True se ptr == 0.
     */
    export function is_null(p: number): boolean;
    /**
     * Le i64 do endereco. UNSAFE: caller garante validade/alinhamento.
     */
    export function read_i64(p: number): number;
    /**
     * Le i32 do endereco e estende para i64.
     */
    export function read_i32(p: number): number;
    /**
     * Le u8 do endereco e estende para i64 (0..255).
     */
    export function read_u8(p: number): number;
    /**
     * Le f64 do endereco.
     */
    export function read_f64(p: number): number;
    /**
     * Escreve i64 no endereco.
     */
    export function write_i64(p: number, value: number): void;
    /**
     * Escreve i32 (low 32 bits) no endereco.
     */
    export function write_i32(p: number, value: number): void;
    /**
     * Escreve u8 (low 8 bits) no endereco.
     */
    export function write_u8(p: number, value: number): void;
    /**
     * Escreve f64 no endereco.
     */
    export function write_f64(p: number, value: number): void;
    /**
     * memmove: copia n bytes de src para dst (overlapping ok).
     */
    export function copy(dst: number, src: number, n: number): void;
    /**
     * memcpy: copia n bytes (regioes nao podem se sobrepor).
     */
    export function copy_nonoverlapping(dst: number, src: number, n: number): void;
    /**
     * memset: preenche n bytes com value (low 8 bits).
     */
    export function write_bytes(dst: number, value: number, n: number): void;
    /**
     * Adiciona n bytes ao ptr.
     */
    export function offset(p: number, n: number): number;
  }

  /**
   * OS and environment info: platform, arch, special directories.
   */
  export namespace os {
    /**
     * Canonical OS name: 'windows', 'linux', 'macos', 'ios', 'android', ...
     */
    export function platform(): string;
    /**
     * CPU architecture: 'x86_64', 'aarch64', 'x86', ...
     */
    export function arch(): string;
    /**
     * OS family: 'unix' or 'windows'.
     */
    export function family(): string;
    /**
     * Native line ending: '\r\n' on Windows, '\n' elsewhere.
     */
    export function eol(): string;
    /**
     * User home directory. Empty string if unresolvable.
     */
    export function home_dir(): string;
    /**
     * System temporary directory.
     */
    export function temp_dir(): string;
    /**
     * Per-user config dir (%APPDATA% / XDG_CONFIG_HOME / ~/.config).
     */
    export function config_dir(): string;
    /**
     * Per-user cache dir (%LOCALAPPDATA% / XDG_CACHE_HOME / ~/.cache).
     */
    export function cache_dir(): string;
  }

  /**
   * Handle-based HashMap and Vec backed by std::collections.
   */
  export namespace collections {
    /**
     * Creates an empty HashMap<string, number> and returns its handle.
     */
    export function map_new(): number;
    /**
     * Releases the map handle.
     */
    export function map_free(h: number): void;
    /**
     * Number of entries; -1 if the handle is invalid.
     */
    export function map_len(h: number): number;
    /**
     * True when the map contains `key`.
     */
    export function map_has(h: number, key: string): boolean;
    /**
     * Value for `key`, or 0 when absent. Use map_has to distinguish.
     */
    export function map_get(h: number, key: string): number;
    /**
     * Inserts/overwrites `key` with `value`.
     */
    export function map_set(h: number, key: string, value: number): void;
    /**
     * Removes `key`. Returns 1 if removed, 0 if absent.
     */
    export function map_delete(h: number, key: string): number;
    /**
     * Removes all entries.
     */
    export function map_clear(h: number): void;
    /**
     * Returns the key at index in deterministic order (sorted). 0 if out of range.
     */
    export function map_key_at(h: number, idx: number): number;
    /**
     * Creates an empty Vec<number>.
     */
    export function vec_new(): number;
    /**
     * Releases the vec handle.
     */
    export function vec_free(h: number): void;
    /**
     * Number of elements; -1 if the handle is invalid.
     */
    export function vec_len(h: number): number;
    /**
     * Appends `value` to the end.
     */
    export function vec_push(h: number, value: number): void;
    /**
     * Removes and returns the last element; 0 when empty.
     */
    export function vec_pop(h: number): number;
    /**
     * Element at `index`, or 0 out of range.
     */
    export function vec_get(h: number, index: number): number;
    /**
     * Writes `value` at `index`. No-op out of range.
     */
    export function vec_set(h: number, index: number, value: number): void;
    /**
     * Removes all elements.
     */
    export function vec_clear(h: number): void;
  }

  /**
   * Non-cryptographic hashing via std::hash::DefaultHasher (SipHash-1-3).
   */
  export namespace hash {
    /**
     * SipHash de uma string UTF-8.
     */
    export function hash_str(s: string): number;
    /**
     * SipHash de uma regiao de memoria (ptr + len). Use com buffer.ptr(handle).
     */
    export function hash_bytes(ptr: number, len: number): number;
    /**
     * SipHash de um inteiro de 64 bits.
     */
    export function hash_i64(value: number): number;
    /**
     * Combina dois hashes preservando entropia (estilo boost::hash_combine).
     */
    export function hash_combine(h1: number, h2: number): number;
  }

  /**
   * Performance hints (std::hint): spin_loop, black_box, unreachable, assert_unchecked.
   */
  export namespace hint {
    /**
     * Hint para spin-wait loop (PAUSE em x86, YIELD em ARM).
     */
    export function spin_loop(): void;
    /**
     * Opaque pra otimizador — impede que o valor seja eliminado.
     */
    export function black_box_i64(value: number): number;
    /**
     * Opaque pra otimizador (variante f64).
     */
    export function black_box_f64(value: number): number;
    /**
     * Marca codigo inalcancavel — em debug aborta, em release eh UB.
     */
    export function unreachable(): never;
    /**
     * Assume cond=true sem verificar. Cond falsa = UB em release.
     */
    export function assert_unchecked(cond: boolean): void;
  }

  /**
   * Parse and format primitives (string <-> number).
   */
  export namespace fmt {
    /**
     * Parses an integer. Returns i64::MIN on error.
     */
    export function parse_i64(s: string): number;
    /**
     * Parses a float. Returns NaN on error.
     */
    export function parse_f64(s: string): number;
    /**
     * Parses 'true'/'false'/'1'/'0' (case-insensitive). Returns -1 on error.
     */
    export function parse_bool(s: string): number;
    /**
     * Decimal string of an integer.
     */
    export function fmt_i64(value: number): string;
    /**
     * Shortest round-trippable decimal of a float.
     */
    export function fmt_f64(value: number): string;
    /**
     * 'true' when value is non-zero, 'false' otherwise.
     */
    export function fmt_bool(value: number): string;
    /**
     * Lowercase hex with `0x` prefix (bits as u64).
     */
    export function fmt_hex(value: number): string;
    /**
     * Binary with `0b` prefix.
     */
    export function fmt_bin(value: number): string;
    /**
     * Octal with `0o` prefix.
     */
    export function fmt_oct(value: number): string;
    /**
     * Float formatted with a fixed number of decimal places.
     */
    export function fmt_f64_prec(value: number, precision: number): string;
  }

  /**
   * Cryptographic primitives: SHA-256, CSPRNG, hex, base64.
   */
  export namespace crypto {
    /**
     * Fills ptr..ptr+len with CSPRNG bytes. 0 ok, -1 err.
     */
    export function random_bytes(ptr: number, len: number): number;
    /**
     * Cryptographically secure i64.
     */
    export function random_i64(): number;
    /**
     * Allocates a buffer of `len` random bytes. Handle 0 on error.
     */
    export function random_buffer(len: number): number;
    /**
     * SHA-256 of a UTF-8 string, returned as a 64-char hex string handle.
     */
    export function sha256_str(s: string): string;
    /**
     * SHA-256 of a raw memory region. Use with buffer.ptr(h).
     */
    export function sha256_bytes(ptr: number, len: number): string;
    /**
     * Lowercase hex of a memory region.
     */
    export function hex_encode(ptr: number, len: number): string;
    /**
     * Decodes hex into a buffer handle. 0 on malformed input.
     */
    export function hex_decode(s: string): number;
    /**
     * Base64 (RFC 4648 padded) of a memory region.
     */
    export function base64_encode(ptr: number, len: number): string;
    /**
     * Decodes base64 into a buffer handle. 0 on malformed input.
     */
    export function base64_decode(s: string): number;
  }

  /**
   * Expressoes regulares via crate `regex` (sintaxe RE2-like, sem backreferences).
   */
  export namespace regex {
    /**
     * Compila um pattern com flags (ex: "i", "gm"). Retorna handle ou 0 se invalido.
     */
    export function compile(pattern: string, flags: string): number;
    /**
     * Libera regex compilada.
     */
    export function free(handle: number): void;
    /**
     * True se a regex casa em qualquer posicao da string.
     */
    export function test(handle: number, s: string): boolean;
    /**
     * Primeira ocorrencia. Retorna handle de string com o match (free pelo caller) ou 0.
     */
    export function find(handle: number, s: string): number;
    /**
     * Indice (em bytes) da primeira ocorrencia, -1 se nao casa.
     */
    export function find_at(handle: number, s: string): number;
    /**
     * Substitui primeira ocorrencia. Retorna handle de string nova.
     */
    export function replace(handle: number, s: string, replacement: string): number;
    /**
     * Substitui todas as ocorrencias. Retorna handle de string nova.
     */
    export function replace_all(handle: number, s: string, replacement: string): number;
    /**
     * Numero de matches (overlapping=false).
     */
    export function match_count(handle: number, s: string): number;
  }

  /**
   * FLTK GUI: windows, widgets, menus, text, drawing, and dialogs.
   */
  export namespace ui {
    /**
     * Creates the FLTK application. Call once before any widget.
     */
    export function app_new(): number;
    /**
     * Enters the FLTK event loop. Blocks until all windows are closed.
     */
    export function app_run(app: number): void;
    /**
     * Frees the app handle.
     */
    export function app_free(app: number): void;
    /**
     * Creates a window (w, h, title). Returns a window handle.
     */
    export function window_new(w: number, h: number, title: string): number;
    /**
     * Makes the window visible.
     */
    export function window_show(win: number): void;
    /**
     * Ends the window widget group. Call after adding all child widgets.
     */
    export function window_end(win: number): void;
    /**
     * Frees the window handle.
     */
    export function window_free(win: number): void;
    /**
     * Sets a callback invoked when the window close button is pressed.
     */
    export function window_set_callback(win: number, fn: () => void): void;
    /**
     * Sets the background color of the window (r, g, b in 0–255).
     */
    export function window_set_color(win: number, r: number, g: number, b: number): void;
    /**
     * Moves and resizes the window (x, y, w, h).
     */
    export function window_resize(win: number, x: number, y: number, w: number, h: number): void;
    /**
     * Sets the label text of any widget.
     */
    export function widget_set_label(widget: number, text: string): void;
    /**
     * Returns the widget label as a GC string handle.
     */
    export function widget_label(widget: number): number;
    /**
     * Registers a callback invoked when the widget is activated (click, change, etc).
     */
    export function widget_set_callback(widget: number, fn: () => void): void;
    /**
     * Variante de set_callback que passa userdata (ex: handle de `this`) ao callback.
     */
    export function widget_set_callback_with_ud(widget: number, fn: (this: number) => void, userdata: number): void;
    /**
     * Sets the background color of a widget (r, g, b in 0–255).
     */
    export function widget_set_color(widget: number, r: number, g: number, b: number): void;
    /**
     * Sets the label text color of a widget (r, g, b in 0–255).
     */
    export function widget_set_label_color(widget: number, r: number, g: number, b: number): void;
    /**
     * Moves and resizes a widget (x, y, w, h).
     */
    export function widget_resize(widget: number, x: number, y: number, w: number, h: number): void;
    /**
     * Marks the widget as needing a redraw.
     */
    export function widget_redraw(widget: number): void;
    /**
     * Hides the widget.
     */
    export function widget_hide(widget: number): void;
    /**
     * Shows the widget.
     */
    export function widget_show(widget: number): void;
    /**
     * Sets a custom draw callback. Use draw_* functions inside to paint the widget.
     */
    export function widget_set_draw(widget: number, fn: () => void): void;
    /**
     * Creates a push button at (x, y) with size (w, h) and label.
     */
    export function button_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Creates a text frame (static label) at (x, y) with size (w, h) and text.
     */
    export function frame_new(x: number, y: number, w: number, h: number, text: string): number;
    /**
     * Creates a checkbox widget.
     */
    export function check_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Returns true if the checkbox is checked.
     */
    export function check_value(handle: number): boolean;
    /**
     * Sets the checked state of a checkbox.
     */
    export function check_set_value(handle: number, val: boolean): void;
    /**
     * Creates a radio button widget.
     */
    export function radio_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Returns true if the radio button is selected.
     */
    export function radio_value(handle: number): boolean;
    /**
     * Sets the selected state of a radio button.
     */
    export function radio_set_value(handle: number, val: boolean): void;
    /**
     * Creates a single-line text input field.
     */
    export function input_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Returns the current text of the input field as a GC string handle.
     */
    export function input_value(handle: number): number;
    /**
     * Sets the text content of an input field.
     */
    export function input_set_value(handle: number, text: string): void;
    /**
     * Creates a read-only output widget.
     */
    export function output_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Sets the text displayed in an output widget.
     */
    export function output_set_value(handle: number, text: string): void;
    /**
     * Creates a horizontal slider.
     */
    export function slider_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Returns the current value of a slider.
     */
    export function slider_value(handle: number): number;
    /**
     * Sets the current value of a slider.
     */
    export function slider_set_value(handle: number, val: number): void;
    /**
     * Sets the minimum and maximum bounds of a slider.
     */
    export function slider_set_bounds(handle: number, min: number, max: number): void;
    /**
     * Creates a progress bar widget.
     */
    export function progress_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Returns the current value of a progress bar.
     */
    export function progress_value(handle: number): number;
    /**
     * Sets the current value of a progress bar.
     */
    export function progress_set_value(handle: number, val: number): void;
    /**
     * Creates a numeric spinner widget.
     */
    export function spinner_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Returns the current numeric value of a spinner.
     */
    export function spinner_value(handle: number): number;
    /**
     * Sets the numeric value of a spinner.
     */
    export function spinner_set_value(handle: number, val: number): void;
    /**
     * Sets the min/max range of a spinner.
     */
    export function spinner_set_bounds(handle: number, min: number, max: number): void;
    /**
     * Creates a menu bar at (x, y) with size (w, h).
     */
    export function menubar_new(x: number, y: number, w: number, h: number): number;
    /**
     * Adds a menu item. Path uses '/' for submenus (e.g. 'File/Open'). fn_ptr=0 for separators.
     */
    export function menubar_add(handle: number, path: string, fn: () => void): void;
    /**
     * Frees a menu bar handle.
     */
    export function menubar_free(handle: number): void;
    /**
     * Creates a text buffer.
     */
    export function textbuf_new(): number;
    /**
     * Replaces the entire text buffer contents.
     */
    export function textbuf_set_text(handle: number, text: string): void;
    /**
     * Returns the text buffer contents as a GC string handle.
     */
    export function textbuf_text(handle: number): number;
    /**
     * Appends text to the end of the buffer.
     */
    export function textbuf_append(handle: number, text: string): void;
    /**
     * Frees the text buffer handle.
     */
    export function textbuf_free(handle: number): void;
    /**
     * Creates a read-only text display widget.
     */
    export function textdisplay_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Associates a TextBuffer with the display.
     */
    export function textdisplay_set_buffer(display: number, buf: number): void;
    /**
     * Creates an editable text editor widget.
     */
    export function texteditor_new(x: number, y: number, w: number, h: number, label: string): number;
    /**
     * Associates a TextBuffer with the editor.
     */
    export function texteditor_set_buffer(editor: number, buf: number): void;
    /**
     * Draws a rectangle outline at (x, y) with size (w, h).
     */
    export function draw_rect(x: number, y: number, w: number, h: number): void;
    /**
     * Draws a filled rectangle at (x, y) with size (w, h).
     */
    export function draw_rect_fill(x: number, y: number, w: number, h: number): void;
    /**
     * Draws a line from (x1, y1) to (x2, y2).
     */
    export function draw_line(x1: number, y1: number, x2: number, y2: number): void;
    /**
     * Draws a circle centered at (x, y) with radius r.
     */
    export function draw_circle(x: number, y: number, r: number): void;
    /**
     * Draws an arc/pie within bounding box (x, y, w, h) from angle a1 to a2 (degrees).
     */
    export function draw_arc(x: number, y: number, w: number, h: number, a1: number, a2: number): void;
    /**
     * Draws text at (x, y) using the current font and color.
     */
    export function draw_text(text: string, x: number, y: number): void;
    /**
     * Sets the current drawing color (r, g, b in 0–255).
     */
    export function set_draw_color(r: number, g: number, b: number): void;
    /**
     * Sets the current font (font_id: 0=Helvetica..14, size in pixels).
     */
    export function set_font(font_id: number, size: number): void;
    /**
     * Sets line style (style: 0=solid,1=dash,2=dot) and width in pixels.
     */
    export function set_line_style(style: number, width: number): void;
    /**
     * Returns the pixel width of a string in the current font.
     */
    export function measure_width(text: string): number;
    /**
     * Shows a blocking alert dialog with a message.
     */
    export function alert(msg: string): void;
    /**
     * Shows a yes/no dialog. Returns true if user clicks Yes.
     */
    export function dialog_ask(msg: string): boolean;
    /**
     * Shows an input dialog (label, default). Returns GC string handle, or 0 on cancel.
     */
    export function dialog_input(label: string, default: string): number;
  }

  /**
   * Dynamic TS/JS evaluation. JIT path uses inline compilation; AOT path spawns rts.
   */
  export namespace runtime {
    /**
     * Evaluates a TS/JS source string. Returns the program exit code.
     */
    export function eval(src: string): number;
    /**
     * Loads and evaluates a TS/JS file at the given path. Returns the program exit code.
     */
    export function eval_file(path: string): number;
  }

  /**
   * Low-level test runner primitives. Use rts:test for the high-level API.
   */
  export namespace test_core {
    /**
     * Opens a named test suite block. Nested calls increase indent.
     */
    export function suite_begin(name: string): void;
    /**
     * Closes the innermost test suite block.
     */
    export function suite_end(): void;
    /**
     * Starts a named test case and resets the failure flag.
     */
    export function case_begin(name: string): void;
    /**
     * Ends the current case. Prints ✓ if no failures, updates counters.
     */
    export function case_end(): void;
    /**
     * Marks current case as failed and emits message in red.
     */
    export function case_fail(msg: string): void;
    /**
     * Marks current case as failed and prints an expected/received diff.
     */
    export function case_fail_diff(expected: string, actual: string): void;
    /**
     * Prints pass/fail counts. Call once at the end of the test file.
     */
    export function print_summary(): void;
  }

}
