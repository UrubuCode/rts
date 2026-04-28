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
    /**
     * Aloca um environment record para closures com `slot_count` slots i64 inicializados em 0. Retorna o handle.
     */
    export function env_alloc(slot_count: number): number;
    /**
     * Lê o slot `slot` do env record. Retorna 0 em handle inválido ou slot fora do range.
     */
    export function env_get(env: number, slot: number): number;
    /**
     * Escreve `value` no slot `slot` do env record. Retorna 1 em sucesso, 0 em erro.
     */
    export function env_set(env: number, slot: number, value: number): number;
    /**
     * Aloca uma instancia com `size` bytes zerados e tag de classe `class_handle`. Retorna o handle ou 0 em erro.
     */
    export function instance_new(size: number, class_handle: number): number;
    /**
     * Retorna o handle da classe (tag string `__rts_class`) da instancia. 0 em handle invalido.
     */
    export function instance_class(handle: number): number;
    /**
     * Libera a instancia. Retorna 1 em sucesso, 0 se ja invalido.
     */
    export function instance_free(handle: number): number;
    /**
     * Le um i64 little-endian no offset (em bytes). 0 em handle invalido ou offset fora do range.
     */
    export function instance_load_i64(handle: number, offset: number): number;
    /**
     * Escreve um i64 little-endian no offset. Retorna 1 em sucesso, 0 em erro.
     */
    export function instance_store_i64(handle: number, offset: number, value: number): number;
    /**
     * Le um i32 little-endian no offset. 0 em handle invalido ou offset fora do range.
     */
    export function instance_load_i32(handle: number, offset: number): number;
    /**
     * Escreve um i32 little-endian no offset. Retorna 1 em sucesso, 0 em erro.
     */
    export function instance_store_i32(handle: number, offset: number, value: number): number;
    /**
     * Le um f64 little-endian no offset. 0.0 em handle invalido ou offset fora do range.
     */
    export function instance_load_f64(handle: number, offset: number): number;
    /**
     * Escreve um f64 little-endian no offset. Retorna 1 em sucesso, 0 em erro.
     */
    export function instance_store_f64(handle: number, offset: number, value: number): number;
    /**
     * Libera o env record. Retorna 1 em sucesso, 0 se handle já inválido.
     */
    export function env_free(env: number): number;
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
   * JSON parsing and serialization (paridade com JSON.parse/stringify).
   */
  export namespace json {
    /**
     * Parses a JSON string into an opaque JSON value handle. Returns 0 on syntax error.
     */
    export function parse(text: string): number;
    /**
     * Serializes a JSON value handle into its compact string form.
     */
    export function stringify(value: number): string;
    /**
     * Pretty-printed serialization with `indent` spaces (>= 0).
     */
    export function stringify_pretty(value: number, indent: number): string;
    /**
     * Releases the JSON value handle.
     */
    export function free(handle: number): void;
    /**
     * Returns: 0 null, 1 bool, 2 number, 3 string, 4 array, 5 object, -1 invalid.
     */
    export function type_of(value: number): number;
    /**
     * Coerces JSON value to bool (true for non-zero/non-null/non-empty).
     */
    export function as_bool(value: number): boolean;
    /**
     * Reads JSON number as i64 (truncates floats). 0 for invalid/non-number.
     */
    export function as_i64(value: number): number;
    /**
     * Reads JSON number as f64. NaN for invalid/non-number.
     */
    export function as_f64(value: number): number;
    /**
     * Reads JSON string as a string handle. Empty handle (0) for non-string.
     */
    export function as_string(value: number): string;
    /**
     * Number of elements when value is array; -1 otherwise.
     */
    export function array_len(value: number): number;
    /**
     * Returns a NEW handle to the element at `index`. 0 if out of range.
     */
    export function array_get(value: number, index: number): number;
    /**
     * Returns a NEW handle to the property `key`. 0 if missing or non-object.
     */
    export function object_get(value: number, key: string): number;
    /**
     * True when value is an object containing `key`.
     */
    export function object_has(value: number, key: string): boolean;
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
    export const PI: number;
    /**
     * Euler's number.
     */
    export const E: number;
    /**
     * Positive infinity.
     */
    export const INFINITY: number;
    /**
     * Quiet NaN.
     */
    export const NAN: number;
  }

  /**
   * TCP/UDP sync sockets backed by std::net.
   */
  export namespace net {
    /**
     * Bind TCP em `addr` ("host:port"). Retorna handle do listener ou 0.
     */
    export function tcp_listen(addr: string): number;
    /**
     * Aceita conexao do listener. Bloqueia. Retorna stream handle ou 0.
     */
    export function tcp_accept(listener: number): number;
    /**
     * Conecta TCP em `addr`. Retorna stream handle ou 0.
     */
    export function tcp_connect(addr: string): number;
    /**
     * Envia bytes da string no stream. Retorna bytes escritos ou -1.
     */
    export function tcp_send(stream: number, data: string): number;
    /**
     * Le ate `len` bytes em `bufPtr`. Retorna bytes lidos (0 = EOF, -1 = erro).
     */
    export function tcp_recv(stream: number, bufPtr: number, len: number): number;
    /**
     * Endereco local do socket (listener ou stream) como string handle. 0 em erro.
     */
    export function tcp_local_addr(handle: number): string;
    /**
     * Fecha o socket TCP (listener ou stream) e libera o handle.
     */
    export function tcp_close(handle: number): void;
    /**
     * Bind UDP em `addr`. Retorna socket handle ou 0.
     */
    export function udp_bind(addr: string): number;
    /**
     * Envia bytes da string em `data` para `dest`. Retorna bytes enviados ou -1.
     */
    export function udp_send_to(sock: number, dest: string, data: string): number;
    /**
     * Le ate `len` bytes em `bufPtr`. Peer endereco fica disponivel via udp_last_peer. Retorna bytes lidos ou -1.
     */
    export function udp_recv_from(sock: number, bufPtr: number, len: number): number;
    /**
     * Endereco do peer da ultima recv_from neste socket. String handle ou 0.
     */
    export function udp_last_peer(sock: number): string;
    /**
     * Endereco local do socket UDP como string handle. 0 em erro.
     */
    export function udp_local_addr(sock: number): string;
    /**
     * Fecha o socket UDP e libera o handle.
     */
    export function udp_close(sock: number): void;
    /**
     * DNS lookup de `host`. Retorna primeiro IP como string handle, ou 0.
     */
    export function resolve(host: string): string;
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
    /**
     * Reinterpreta os bits de um i64 como f64 (bit-cast). Util pra recuperar f64 de canais que so passam i64 (ex: thread.spawn arg).
     */
    export function f64_from_bits(bits: number): number;
    /**
     * Reinterpreta os bits de um f64 como i64 (bit-cast). Util pra serializar f64 em canais i64-only.
     */
    export function f64_to_bits(value: number): number;
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
   * RTS stack trace and debug tooling. Push/pop TS call frames; capture trace without error.
   */
  export namespace trace {
    /**
     * Push a TS call frame onto the trace stack.
     */
    export function push_frame(file: string, fn_name: string, line: number, col: number): void;
    /**
     * Pop the top TS call frame from the trace stack.
     */
    export function pop_frame(): void;
    /**
     * Capture current trace as a GC string handle. Returns 0 if stack is empty.
     */
    export function capture(): number;
    /**
     * Print current trace stack to stderr.
     */
    export function print(): void;
    /**
     * Returns current trace stack depth.
     */
    export function depth(): number;
    /**
     * Free a captured trace handle.
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
   * C-string and OS-string interop via std::ffi (CStr/CString/OsStr/OsString).
   */
  export namespace ffi {
    /**
     * Reads a nul-terminated C string from `ptr` and returns a string handle (UTF-8 lossy).
     */
    export function cstr_from_ptr(ptr: number): number;
    /**
     * Length in bytes of the C string at `ptr`, excluding the nul terminator. -1 if ptr is null.
     */
    export function cstr_len(ptr: number): number;
    /**
     * Validates the C string at `ptr` as UTF-8 and returns a string handle. 0 if invalid.
     */
    export function cstr_to_str(ptr: number): number;
    /**
     * Builds a nul-terminated CString from `s` and returns a handle. 0 if `s` contains an interior nul.
     */
    export function cstring_new(s: string): number;
    /**
     * Raw pointer to the CString bytes (nul-terminated). 0 if handle invalid. Unsafe — must not outlive handle.
     */
    export function cstring_ptr(handle: number): number;
    /**
     * Releases the CString handle.
     */
    export function cstring_free(handle: number): void;
    /**
     * Builds an OsString from a UTF-8 source and returns a handle.
     */
    export function osstr_from_str(s: string): number;
    /**
     * Converts the OsString handle to a UTF-8 string handle. 0 if not valid UTF-8.
     */
    export function osstr_to_str(handle: number): number;
    /**
     * Releases the OsString handle.
     */
    export function osstr_free(handle: number): void;
  }

  /**
   * Primitivas atomicas (AtomicI64, AtomicBool, fences) baseadas em std::sync::atomic.
   */
  export namespace atomic {
    /**
     * Aloca um AtomicI64 inicializado com `value` e retorna o handle.
     */
    export function i64_new(value: number): number;
    /**
     * Le o valor atual do AtomicI64 (SeqCst). 0 se handle invalido.
     */
    export function i64_load(handle: number): number;
    /**
     * Escreve `value` no AtomicI64 (SeqCst). No-op se handle invalido.
     */
    export function i64_store(handle: number, value: number): void;
    /**
     * Soma `delta` e retorna o valor anterior. 0 se handle invalido.
     */
    export function i64_fetch_add(handle: number, delta: number): number;
    /**
     * Subtrai `delta` e retorna o valor anterior. 0 se handle invalido.
     */
    export function i64_fetch_sub(handle: number, delta: number): number;
    /**
     * AND bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
     */
    export function i64_fetch_and(handle: number, mask: number): number;
    /**
     * OR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
     */
    export function i64_fetch_or(handle: number, mask: number): number;
    /**
     * XOR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
     */
    export function i64_fetch_xor(handle: number, mask: number): number;
    /**
     * Troca o valor por `value` e retorna o valor anterior. 0 se handle invalido.
     */
    export function i64_swap(handle: number, value: number): number;
    /**
     * Compare-and-swap. Se valor atual == `expected`, escreve `new`. Retorna o valor anterior.
     */
    export function i64_cas(handle: number, expected: number, new_value: number): number;
    /**
     * Aloca um AtomicBool inicializado com `value` e retorna o handle.
     */
    export function bool_new(value: boolean): number;
    /**
     * Le o valor atual do AtomicBool (SeqCst). false se handle invalido.
     */
    export function bool_load(handle: number): boolean;
    /**
     * Escreve `value` no AtomicBool (SeqCst). No-op se handle invalido.
     */
    export function bool_store(handle: number, value: boolean): void;
    /**
     * Troca o valor por `value` e retorna o valor anterior. false se handle invalido.
     */
    export function bool_swap(handle: number, value: boolean): boolean;
    /**
     * Aloca um AtomicF64 inicializado com `value` e retorna o handle.
     */
    export function f64_new(value: number): number;
    /**
     * Le o valor atual do AtomicF64 (SeqCst). 0.0 se handle invalido.
     */
    export function f64_load(handle: number): number;
    /**
     * Escreve `value` no AtomicF64 (SeqCst). No-op se handle invalido.
     */
    export function f64_store(handle: number, value: number): void;
    /**
     * Soma `delta` e retorna o valor anterior (loop CAS internamente). 0.0 se handle invalido.
     */
    export function f64_fetch_add(handle: number, delta: number): number;
    /**
     * Troca o valor por `value` e retorna o valor anterior. 0.0 se handle invalido.
     */
    export function f64_swap(handle: number, value: number): number;
    /**
     * Memory fence Acquire.
     */
    export function fence_acquire(): void;
    /**
     * Memory fence Release.
     */
    export function fence_release(): void;
    /**
     * Memory fence SeqCst.
     */
    export function fence_seq_cst(): void;
  }

  /**
   * Primitivas de sincronizacao (Mutex, RwLock, OnceLock) baseadas em std::sync.
   */
  export namespace sync {
    /**
     * Aloca um Mutex protegendo um valor i64 inicializado com `initial`.
     */
    export function mutex_new(initial: number): number;
    /**
     * Bloqueia ate adquirir o Mutex e retorna o valor interno protegido. 0 se handle invalido.
     */
    export function mutex_lock(mutex: number): number;
    /**
     * Tenta adquirir o Mutex sem bloquear. Retorna o valor interno em caso de sucesso, 0 se ja estava lockado ou handle invalido.
     */
    export function mutex_try_lock(mutex: number): number;
    /**
     * Escreve `value` no Mutex. Caller deve ter chamado lock/try_lock antes (responsabilidade do caller).
     */
    export function mutex_set(mutex: number, value: number): void;
    /**
     * Libera o Mutex previamente adquirido por lock/try_lock. No-op se nao havia guard ativo.
     */
    export function mutex_unlock(mutex: number): void;
    /**
     * Libera o Mutex e seu slot na HandleTable.
     */
    export function mutex_free(mutex: number): void;
    /**
     * Aloca um RwLock protegendo um valor i64 inicializado com `initial`.
     */
    export function rwlock_new(initial: number): number;
    /**
     * Adquire um read guard (compartilhado) e retorna um handle de guard. Liberar via rwlock_unlock(guard). 0 se handle invalido.
     */
    export function rwlock_read(rwlock: number): number;
    /**
     * Adquire um write guard (exclusivo) e retorna um handle de guard. Liberar via rwlock_unlock(guard). 0 se handle invalido.
     */
    export function rwlock_write(rwlock: number): number;
    /**
     * Libera um guard previamente adquirido via rwlock_read/rwlock_write.
     */
    export function rwlock_unlock(guard: number): void;
    /**
     * Aloca um OnceLock e retorna o handle.
     */
    export function once_new(): number;
    /**
     * Executa `fn_ptr` (ponteiro para `extern "C" fn()`) exatamente uma vez por OnceLock. Chamadas subsequentes sao no-op.
     */
    export function once_call(once: number, fn_ptr: number): void;
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
     * Shows an input dialog (label, def). Returns GC string handle, or 0 on cancel.
     */
    export function dialog_input(label: string, def: string): number;
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

  /**
   * Primitivas de threads (spawn/join/detach/scope/id/sleep) baseadas em std::thread.
   */
  export namespace thread {
    /**
     * Cria uma nova thread executando `fn_ptr(arg)`. `fn_ptr` e um ponteiro para `extern "C" fn(u64) -> u64`. Retorna handle do JoinHandle, 0 em falha.
     */
    export function spawn(fn_ptr: number, arg: number): number;
    /**
     * Variante de `spawn` que passa um `userdata` (ex: handle de `this`) como primeiro argumento da fn. `fn_ptr` aponta para `extern "C" fn(u64, u64) -> u64` (ud, arg).
     */
    export function spawn_with_ud(fn_ptr: number, arg: number, userdata: number): number;
    /**
     * Roda `body()` num escopo que aguarda automaticamente todas as threads spawnadas durante sua execucao. Analogo a `std::thread::scope`.
     */
    export function scope(body: () => void): void;
    /**
     * Variante com userdata para `thread.scope` quando o body captura `this`.
     */
    export function scope_with_ud(body: number, userdata: number): void;
    /**
     * Aguarda a thread terminar e retorna o valor retornado por ela. Consome o handle. 0 se handle invalido ou a thread fez panic.
     */
    export function join(thread: number): number;
    /**
     * Libera o JoinHandle sem aguardar. A thread continua rodando ate completar.
     */
    export function detach(thread: number): void;
    /**
     * Id da thread atual (estavel por thread, atribuido na primeira chamada). Sempre != 0.
     */
    export function id(): number;
    /**
     * Pausa a thread atual por `ms` milissegundos. Valores negativos sao tratados como 0.
     */
    export function sleep_ms(ms: number): void;
  }

  /**
   * Paralelismo de dados via Rayon (map/for_each/reduce sobre Vec<i64>).
   */
  export namespace parallel {
    /**
     * Aplica `fn_ptr(x)` em paralelo sobre cada elemento do Vec<i64> `vec_handle`. Retorna novo Vec<i64> com os resultados. `fn_ptr` e `extern "C" fn(i64) -> i64`.
     */
    export function map(vec_handle: number, fn_ptr: number): number;
    /**
     * Executa `fn_ptr(x)` em paralelo para cada elemento do Vec<i64> `vec_handle`. `fn_ptr` e `extern "C" fn(i64)`.
     */
    export function for_each(vec_handle: number, fn_ptr: number): void;
    /**
     * Reduz Vec<i64> `vec_handle` com `fn_ptr(acc, x) -> acc` em paralelo (divide-e-conquista). `identity` e o elemento neutro da operacao (0 para soma, 1 para produto). `fn_ptr` deve ser associativo.
     */
    export function reduce(vec_handle: number, identity: number, fn_ptr: number): number;
    /**
     * Retorna o numero de threads no pool Rayon global.
     */
    export function num_threads(): number;
  }

  /**
   * TLS 1.2/1.3 client sync via rustls (HTTPS support).
   */
  export namespace tls {
    /**
     * Wraps tcp_handle numa conexao TLS client. Consome o tcp_handle. SNI = sni_hostname. Retorna stream handle ou 0 (handshake falhou).
     */
    export function client(tcp: number, sniHostname: string): number;
    /**
     * Envia bytes encriptados pelo TLS stream. Retorna bytes plain enviados ou -1.
     */
    export function send(stream: number, data: string): number;
    /**
     * Le ate `len` bytes plain do TLS stream. Retorna bytes lidos (0 = EOF, -1 = erro).
     */
    export function recv(stream: number, bufPtr: number, len: number): number;
    /**
     * Fecha o stream TLS (close_notify) e libera o handle.
     */
    export function close(stream: number): void;
  }

}

declare module "rts:gc" {
  /**
   * Runtime-managed handle table and string pool.
   */
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
  /**
   * Aloca um environment record para closures com `slot_count` slots i64 inicializados em 0. Retorna o handle.
   */
  export function env_alloc(slot_count: number): number;
  /**
   * Lê o slot `slot` do env record. Retorna 0 em handle inválido ou slot fora do range.
   */
  export function env_get(env: number, slot: number): number;
  /**
   * Escreve `value` no slot `slot` do env record. Retorna 1 em sucesso, 0 em erro.
   */
  export function env_set(env: number, slot: number, value: number): number;
  /**
   * Aloca uma instancia com `size` bytes zerados e tag de classe `class_handle`. Retorna o handle ou 0 em erro.
   */
  export function instance_new(size: number, class_handle: number): number;
  /**
   * Retorna o handle da classe (tag string `__rts_class`) da instancia. 0 em handle invalido.
   */
  export function instance_class(handle: number): number;
  /**
   * Libera a instancia. Retorna 1 em sucesso, 0 se ja invalido.
   */
  export function instance_free(handle: number): number;
  /**
   * Le um i64 little-endian no offset (em bytes). 0 em handle invalido ou offset fora do range.
   */
  export function instance_load_i64(handle: number, offset: number): number;
  /**
   * Escreve um i64 little-endian no offset. Retorna 1 em sucesso, 0 em erro.
   */
  export function instance_store_i64(handle: number, offset: number, value: number): number;
  /**
   * Le um i32 little-endian no offset. 0 em handle invalido ou offset fora do range.
   */
  export function instance_load_i32(handle: number, offset: number): number;
  /**
   * Escreve um i32 little-endian no offset. Retorna 1 em sucesso, 0 em erro.
   */
  export function instance_store_i32(handle: number, offset: number, value: number): number;
  /**
   * Le um f64 little-endian no offset. 0.0 em handle invalido ou offset fora do range.
   */
  export function instance_load_f64(handle: number, offset: number): number;
  /**
   * Escreve um f64 little-endian no offset. Retorna 1 em sucesso, 0 em erro.
   */
  export function instance_store_f64(handle: number, offset: number, value: number): number;
  /**
   * Libera o env record. Retorna 1 em sucesso, 0 se handle já inválido.
   */
  export function env_free(env: number): number;
  const _default: {
    string_from_i64: (typeof import("rts"))["gc"]["string_from_i64"];
    string_from_f64: (typeof import("rts"))["gc"]["string_from_f64"];
    string_concat: (typeof import("rts"))["gc"]["string_concat"];
    string_eq: (typeof import("rts"))["gc"]["string_eq"];
    string_from_static: (typeof import("rts"))["gc"]["string_from_static"];
    string_new: (typeof import("rts"))["gc"]["string_new"];
    string_len: (typeof import("rts"))["gc"]["string_len"];
    string_ptr: (typeof import("rts"))["gc"]["string_ptr"];
    string_free: (typeof import("rts"))["gc"]["string_free"];
    env_alloc: (typeof import("rts"))["gc"]["env_alloc"];
    env_get: (typeof import("rts"))["gc"]["env_get"];
    env_set: (typeof import("rts"))["gc"]["env_set"];
    instance_new: (typeof import("rts"))["gc"]["instance_new"];
    instance_class: (typeof import("rts"))["gc"]["instance_class"];
    instance_free: (typeof import("rts"))["gc"]["instance_free"];
    instance_load_i64: (typeof import("rts"))["gc"]["instance_load_i64"];
    instance_store_i64: (typeof import("rts"))["gc"]["instance_store_i64"];
    instance_load_i32: (typeof import("rts"))["gc"]["instance_load_i32"];
    instance_store_i32: (typeof import("rts"))["gc"]["instance_store_i32"];
    instance_load_f64: (typeof import("rts"))["gc"]["instance_load_f64"];
    instance_store_f64: (typeof import("rts"))["gc"]["instance_store_f64"];
    env_free: (typeof import("rts"))["gc"]["env_free"];
  };
  export default _default;
}

declare module "rts:io" {
  /**
   * Standard input/output primitives backed by std::io.
   */
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
  const _default: {
    print: (typeof import("rts"))["io"]["print"];
    eprint: (typeof import("rts"))["io"]["eprint"];
    stdout_write: (typeof import("rts"))["io"]["stdout_write"];
    stdout_flush: (typeof import("rts"))["io"]["stdout_flush"];
    stderr_write: (typeof import("rts"))["io"]["stderr_write"];
    stderr_flush: (typeof import("rts"))["io"]["stderr_flush"];
    stdin_read: (typeof import("rts"))["io"]["stdin_read"];
    stdin_read_line: (typeof import("rts"))["io"]["stdin_read_line"];
  };
  export default _default;
}

declare module "rts:json" {
  /**
   * JSON parsing and serialization (paridade com JSON.parse/stringify).
   */
  /**
   * Parses a JSON string into an opaque JSON value handle. Returns 0 on syntax error.
   */
  export function parse(text: string): number;
  /**
   * Serializes a JSON value handle into its compact string form.
   */
  export function stringify(value: number): string;
  /**
   * Pretty-printed serialization with `indent` spaces (>= 0).
   */
  export function stringify_pretty(value: number, indent: number): string;
  /**
   * Releases the JSON value handle.
   */
  export function free(handle: number): void;
  /**
   * Returns: 0 null, 1 bool, 2 number, 3 string, 4 array, 5 object, -1 invalid.
   */
  export function type_of(value: number): number;
  /**
   * Coerces JSON value to bool (true for non-zero/non-null/non-empty).
   */
  export function as_bool(value: number): boolean;
  /**
   * Reads JSON number as i64 (truncates floats). 0 for invalid/non-number.
   */
  export function as_i64(value: number): number;
  /**
   * Reads JSON number as f64. NaN for invalid/non-number.
   */
  export function as_f64(value: number): number;
  /**
   * Reads JSON string as a string handle. Empty handle (0) for non-string.
   */
  export function as_string(value: number): string;
  /**
   * Number of elements when value is array; -1 otherwise.
   */
  export function array_len(value: number): number;
  /**
   * Returns a NEW handle to the element at `index`. 0 if out of range.
   */
  export function array_get(value: number, index: number): number;
  /**
   * Returns a NEW handle to the property `key`. 0 if missing or non-object.
   */
  export function object_get(value: number, key: string): number;
  /**
   * True when value is an object containing `key`.
   */
  export function object_has(value: number, key: string): boolean;
  const _default: {
    parse: (typeof import("rts"))["json"]["parse"];
    stringify: (typeof import("rts"))["json"]["stringify"];
    stringify_pretty: (typeof import("rts"))["json"]["stringify_pretty"];
    free: (typeof import("rts"))["json"]["free"];
    type_of: (typeof import("rts"))["json"]["type_of"];
    as_bool: (typeof import("rts"))["json"]["as_bool"];
    as_i64: (typeof import("rts"))["json"]["as_i64"];
    as_f64: (typeof import("rts"))["json"]["as_f64"];
    as_string: (typeof import("rts"))["json"]["as_string"];
    array_len: (typeof import("rts"))["json"]["array_len"];
    array_get: (typeof import("rts"))["json"]["array_get"];
    object_get: (typeof import("rts"))["json"]["object_get"];
    object_has: (typeof import("rts"))["json"]["object_has"];
  };
  export default _default;
}

declare module "rts:fs" {
  /**
   * Filesystem operations backed by std::fs.
   */
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
  const _default: {
    read: (typeof import("rts"))["fs"]["read"];
    read_all: (typeof import("rts"))["fs"]["read_all"];
    write: (typeof import("rts"))["fs"]["write"];
    append: (typeof import("rts"))["fs"]["append"];
    exists: (typeof import("rts"))["fs"]["exists"];
    is_file: (typeof import("rts"))["fs"]["is_file"];
    is_dir: (typeof import("rts"))["fs"]["is_dir"];
    size: (typeof import("rts"))["fs"]["size"];
    modified_ms: (typeof import("rts"))["fs"]["modified_ms"];
    create_dir: (typeof import("rts"))["fs"]["create_dir"];
    create_dir_all: (typeof import("rts"))["fs"]["create_dir_all"];
    remove_dir: (typeof import("rts"))["fs"]["remove_dir"];
    remove_dir_all: (typeof import("rts"))["fs"]["remove_dir_all"];
    remove_file: (typeof import("rts"))["fs"]["remove_file"];
    rename: (typeof import("rts"))["fs"]["rename"];
    copy: (typeof import("rts"))["fs"]["copy"];
  };
  export default _default;
}

declare module "rts:math" {
  /**
   * Floating-point / integer intrinsics and a seeded xorshift PRNG.
   */
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
  export const PI: number;
  /**
   * Euler's number.
   */
  export const E: number;
  /**
   * Positive infinity.
   */
  export const INFINITY: number;
  /**
   * Quiet NaN.
   */
  export const NAN: number;
  const _default: {
    floor: (typeof import("rts"))["math"]["floor"];
    ceil: (typeof import("rts"))["math"]["ceil"];
    round: (typeof import("rts"))["math"]["round"];
    trunc: (typeof import("rts"))["math"]["trunc"];
    sqrt: (typeof import("rts"))["math"]["sqrt"];
    cbrt: (typeof import("rts"))["math"]["cbrt"];
    pow: (typeof import("rts"))["math"]["pow"];
    exp: (typeof import("rts"))["math"]["exp"];
    ln: (typeof import("rts"))["math"]["ln"];
    log2: (typeof import("rts"))["math"]["log2"];
    log10: (typeof import("rts"))["math"]["log10"];
    abs_f64: (typeof import("rts"))["math"]["abs_f64"];
    abs_i64: (typeof import("rts"))["math"]["abs_i64"];
    sin: (typeof import("rts"))["math"]["sin"];
    cos: (typeof import("rts"))["math"]["cos"];
    tan: (typeof import("rts"))["math"]["tan"];
    asin: (typeof import("rts"))["math"]["asin"];
    acos: (typeof import("rts"))["math"]["acos"];
    atan: (typeof import("rts"))["math"]["atan"];
    atan2: (typeof import("rts"))["math"]["atan2"];
    min_f64: (typeof import("rts"))["math"]["min_f64"];
    max_f64: (typeof import("rts"))["math"]["max_f64"];
    min_i64: (typeof import("rts"))["math"]["min_i64"];
    max_i64: (typeof import("rts"))["math"]["max_i64"];
    clamp_f64: (typeof import("rts"))["math"]["clamp_f64"];
    clamp_i64: (typeof import("rts"))["math"]["clamp_i64"];
    random_f64: (typeof import("rts"))["math"]["random_f64"];
    random_i64_range: (typeof import("rts"))["math"]["random_i64_range"];
    seed: (typeof import("rts"))["math"]["seed"];
    PI: (typeof import("rts"))["math"]["PI"];
    E: (typeof import("rts"))["math"]["E"];
    INFINITY: (typeof import("rts"))["math"]["INFINITY"];
    NAN: (typeof import("rts"))["math"]["NAN"];
  };
  export default _default;
}

declare module "rts:net" {
  /**
   * TCP/UDP sync sockets backed by std::net.
   */
  /**
   * Bind TCP em `addr` ("host:port"). Retorna handle do listener ou 0.
   */
  export function tcp_listen(addr: string): number;
  /**
   * Aceita conexao do listener. Bloqueia. Retorna stream handle ou 0.
   */
  export function tcp_accept(listener: number): number;
  /**
   * Conecta TCP em `addr`. Retorna stream handle ou 0.
   */
  export function tcp_connect(addr: string): number;
  /**
   * Envia bytes da string no stream. Retorna bytes escritos ou -1.
   */
  export function tcp_send(stream: number, data: string): number;
  /**
   * Le ate `len` bytes em `bufPtr`. Retorna bytes lidos (0 = EOF, -1 = erro).
   */
  export function tcp_recv(stream: number, bufPtr: number, len: number): number;
  /**
   * Endereco local do socket (listener ou stream) como string handle. 0 em erro.
   */
  export function tcp_local_addr(handle: number): string;
  /**
   * Fecha o socket TCP (listener ou stream) e libera o handle.
   */
  export function tcp_close(handle: number): void;
  /**
   * Bind UDP em `addr`. Retorna socket handle ou 0.
   */
  export function udp_bind(addr: string): number;
  /**
   * Envia bytes da string em `data` para `dest`. Retorna bytes enviados ou -1.
   */
  export function udp_send_to(sock: number, dest: string, data: string): number;
  /**
   * Le ate `len` bytes em `bufPtr`. Peer endereco fica disponivel via udp_last_peer. Retorna bytes lidos ou -1.
   */
  export function udp_recv_from(sock: number, bufPtr: number, len: number): number;
  /**
   * Endereco do peer da ultima recv_from neste socket. String handle ou 0.
   */
  export function udp_last_peer(sock: number): string;
  /**
   * Endereco local do socket UDP como string handle. 0 em erro.
   */
  export function udp_local_addr(sock: number): string;
  /**
   * Fecha o socket UDP e libera o handle.
   */
  export function udp_close(sock: number): void;
  /**
   * DNS lookup de `host`. Retorna primeiro IP como string handle, ou 0.
   */
  export function resolve(host: string): string;
  const _default: {
    tcp_listen: (typeof import("rts"))["net"]["tcp_listen"];
    tcp_accept: (typeof import("rts"))["net"]["tcp_accept"];
    tcp_connect: (typeof import("rts"))["net"]["tcp_connect"];
    tcp_send: (typeof import("rts"))["net"]["tcp_send"];
    tcp_recv: (typeof import("rts"))["net"]["tcp_recv"];
    tcp_local_addr: (typeof import("rts"))["net"]["tcp_local_addr"];
    tcp_close: (typeof import("rts"))["net"]["tcp_close"];
    udp_bind: (typeof import("rts"))["net"]["udp_bind"];
    udp_send_to: (typeof import("rts"))["net"]["udp_send_to"];
    udp_recv_from: (typeof import("rts"))["net"]["udp_recv_from"];
    udp_last_peer: (typeof import("rts"))["net"]["udp_last_peer"];
    udp_local_addr: (typeof import("rts"))["net"]["udp_local_addr"];
    udp_close: (typeof import("rts"))["net"]["udp_close"];
    resolve: (typeof import("rts"))["net"]["resolve"];
  };
  export default _default;
}

declare module "rts:num" {
  /**
   * Aritmetica com overflow explicito (checked/saturating/wrapping) e bit ops.
   */
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
  /**
   * Reinterpreta os bits de um i64 como f64 (bit-cast). Util pra recuperar f64 de canais que so passam i64 (ex: thread.spawn arg).
   */
  export function f64_from_bits(bits: number): number;
  /**
   * Reinterpreta os bits de um f64 como i64 (bit-cast). Util pra serializar f64 em canais i64-only.
   */
  export function f64_to_bits(value: number): number;
  const _default: {
    checked_add: (typeof import("rts"))["num"]["checked_add"];
    checked_sub: (typeof import("rts"))["num"]["checked_sub"];
    checked_mul: (typeof import("rts"))["num"]["checked_mul"];
    checked_div: (typeof import("rts"))["num"]["checked_div"];
    saturating_add: (typeof import("rts"))["num"]["saturating_add"];
    saturating_sub: (typeof import("rts"))["num"]["saturating_sub"];
    saturating_mul: (typeof import("rts"))["num"]["saturating_mul"];
    wrapping_add: (typeof import("rts"))["num"]["wrapping_add"];
    wrapping_sub: (typeof import("rts"))["num"]["wrapping_sub"];
    wrapping_mul: (typeof import("rts"))["num"]["wrapping_mul"];
    wrapping_neg: (typeof import("rts"))["num"]["wrapping_neg"];
    wrapping_shl: (typeof import("rts"))["num"]["wrapping_shl"];
    wrapping_shr: (typeof import("rts"))["num"]["wrapping_shr"];
    count_ones: (typeof import("rts"))["num"]["count_ones"];
    count_zeros: (typeof import("rts"))["num"]["count_zeros"];
    leading_zeros: (typeof import("rts"))["num"]["leading_zeros"];
    trailing_zeros: (typeof import("rts"))["num"]["trailing_zeros"];
    rotate_left: (typeof import("rts"))["num"]["rotate_left"];
    rotate_right: (typeof import("rts"))["num"]["rotate_right"];
    reverse_bits: (typeof import("rts"))["num"]["reverse_bits"];
    swap_bytes: (typeof import("rts"))["num"]["swap_bytes"];
    f64_from_bits: (typeof import("rts"))["num"]["f64_from_bits"];
    f64_to_bits: (typeof import("rts"))["num"]["f64_to_bits"];
  };
  export default _default;
}

declare module "rts:mem" {
  /**
   * std::mem: layout (size_of/align_of), swap, drop, forget.
   */
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
  const _default: {
    size_of_i64: (typeof import("rts"))["mem"]["size_of_i64"];
    size_of_f64: (typeof import("rts"))["mem"]["size_of_f64"];
    size_of_i32: (typeof import("rts"))["mem"]["size_of_i32"];
    size_of_bool: (typeof import("rts"))["mem"]["size_of_bool"];
    align_of_i64: (typeof import("rts"))["mem"]["align_of_i64"];
    align_of_f64: (typeof import("rts"))["mem"]["align_of_f64"];
    swap_i64: (typeof import("rts"))["mem"]["swap_i64"];
    drop_handle: (typeof import("rts"))["mem"]["drop_handle"];
    forget_handle: (typeof import("rts"))["mem"]["forget_handle"];
    replace_i64: (typeof import("rts"))["mem"]["replace_i64"];
  };
  export default _default;
}

declare module "rts:trace" {
  /**
   * RTS stack trace and debug tooling. Push/pop TS call frames; capture trace without error.
   */
  /**
   * Push a TS call frame onto the trace stack.
   */
  export function push_frame(file: string, fn_name: string, line: number, col: number): void;
  /**
   * Pop the top TS call frame from the trace stack.
   */
  export function pop_frame(): void;
  /**
   * Capture current trace as a GC string handle. Returns 0 if stack is empty.
   */
  export function capture(): number;
  /**
   * Print current trace stack to stderr.
   */
  export function print(): void;
  /**
   * Returns current trace stack depth.
   */
  export function depth(): number;
  /**
   * Free a captured trace handle.
   */
  export function free(handle: number): void;
  const _default: {
    push_frame: (typeof import("rts"))["trace"]["push_frame"];
    pop_frame: (typeof import("rts"))["trace"]["pop_frame"];
    capture: (typeof import("rts"))["trace"]["capture"];
    print: (typeof import("rts"))["trace"]["print"];
    depth: (typeof import("rts"))["trace"]["depth"];
    free: (typeof import("rts"))["trace"]["free"];
  };
  export default _default;
}

declare module "rts:alloc" {
  /**
   * Allocator raw via std::alloc. UNSAFE — pareie alloc/dealloc com mesmo size/align.
   */
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
  const _default: {
    alloc: (typeof import("rts"))["alloc"]["alloc"];
    alloc_zeroed: (typeof import("rts"))["alloc"]["alloc_zeroed"];
    dealloc: (typeof import("rts"))["alloc"]["dealloc"];
    realloc: (typeof import("rts"))["alloc"]["realloc"];
  };
  export default _default;
}

declare module "rts:bigfloat" {
  /**
   * Arbitrary-precision decimal floating-point via handle table.
   */
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
  const _default: {
    zero: (typeof import("rts"))["bigfloat"]["zero"];
    from_f64: (typeof import("rts"))["bigfloat"]["from_f64"];
    from_str: (typeof import("rts"))["bigfloat"]["from_str"];
    from_i64: (typeof import("rts"))["bigfloat"]["from_i64"];
    to_f64: (typeof import("rts"))["bigfloat"]["to_f64"];
    to_string: (typeof import("rts"))["bigfloat"]["to_string"];
    add: (typeof import("rts"))["bigfloat"]["add"];
    sub: (typeof import("rts"))["bigfloat"]["sub"];
    mul: (typeof import("rts"))["bigfloat"]["mul"];
    div: (typeof import("rts"))["bigfloat"]["div"];
    neg: (typeof import("rts"))["bigfloat"]["neg"];
    sqrt: (typeof import("rts"))["bigfloat"]["sqrt"];
    free: (typeof import("rts"))["bigfloat"]["free"];
  };
  export default _default;
}

declare module "rts:time" {
  /**
   * Monotonic and wall-clock timestamps, plus blocking sleeps.
   */
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
  const _default: {
    now_ms: (typeof import("rts"))["time"]["now_ms"];
    now_ns: (typeof import("rts"))["time"]["now_ns"];
    unix_ms: (typeof import("rts"))["time"]["unix_ms"];
    unix_ns: (typeof import("rts"))["time"]["unix_ns"];
    sleep_ms: (typeof import("rts"))["time"]["sleep_ms"];
    sleep_ns: (typeof import("rts"))["time"]["sleep_ns"];
  };
  export default _default;
}

declare module "rts:env" {
  /**
   * Environment variables, process argv, and current working directory.
   */
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
  const _default: {
    get_var: (typeof import("rts"))["env"]["get_var"];
    set_var: (typeof import("rts"))["env"]["set_var"];
    remove_var: (typeof import("rts"))["env"]["remove_var"];
    args_count: (typeof import("rts"))["env"]["args_count"];
    arg_at: (typeof import("rts"))["env"]["arg_at"];
    cwd: (typeof import("rts"))["env"]["cwd"];
    set_cwd: (typeof import("rts"))["env"]["set_cwd"];
  };
  export default _default;
}

declare module "rts:path" {
  /**
   * Pure path manipulation — no filesystem calls.
   */
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
  const _default: {
    join: (typeof import("rts"))["path"]["join"];
    parent: (typeof import("rts"))["path"]["parent"];
    file_name: (typeof import("rts"))["path"]["file_name"];
    stem: (typeof import("rts"))["path"]["stem"];
    ext: (typeof import("rts"))["path"]["ext"];
    is_absolute: (typeof import("rts"))["path"]["is_absolute"];
    normalize: (typeof import("rts"))["path"]["normalize"];
    with_ext: (typeof import("rts"))["path"]["with_ext"];
  };
  export default _default;
}

declare module "rts:buffer" {
  /**
   * Binary buffers backed by Vec<u8> in the handle table.
   */
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
  const _default: {
    alloc: (typeof import("rts"))["buffer"]["alloc"];
    alloc_zeroed: (typeof import("rts"))["buffer"]["alloc_zeroed"];
    free: (typeof import("rts"))["buffer"]["free"];
    len: (typeof import("rts"))["buffer"]["len"];
    ptr: (typeof import("rts"))["buffer"]["ptr"];
    read_u8: (typeof import("rts"))["buffer"]["read_u8"];
    read_i32: (typeof import("rts"))["buffer"]["read_i32"];
    read_i64: (typeof import("rts"))["buffer"]["read_i64"];
    read_f64: (typeof import("rts"))["buffer"]["read_f64"];
    write_u8: (typeof import("rts"))["buffer"]["write_u8"];
    write_i32: (typeof import("rts"))["buffer"]["write_i32"];
    write_i64: (typeof import("rts"))["buffer"]["write_i64"];
    write_f64: (typeof import("rts"))["buffer"]["write_f64"];
    copy: (typeof import("rts"))["buffer"]["copy"];
    fill: (typeof import("rts"))["buffer"]["fill"];
    to_string: (typeof import("rts"))["buffer"]["to_string"];
  };
  export default _default;
}

declare module "rts:ffi" {
  /**
   * C-string and OS-string interop via std::ffi (CStr/CString/OsStr/OsString).
   */
  /**
   * Reads a nul-terminated C string from `ptr` and returns a string handle (UTF-8 lossy).
   */
  export function cstr_from_ptr(ptr: number): number;
  /**
   * Length in bytes of the C string at `ptr`, excluding the nul terminator. -1 if ptr is null.
   */
  export function cstr_len(ptr: number): number;
  /**
   * Validates the C string at `ptr` as UTF-8 and returns a string handle. 0 if invalid.
   */
  export function cstr_to_str(ptr: number): number;
  /**
   * Builds a nul-terminated CString from `s` and returns a handle. 0 if `s` contains an interior nul.
   */
  export function cstring_new(s: string): number;
  /**
   * Raw pointer to the CString bytes (nul-terminated). 0 if handle invalid. Unsafe — must not outlive handle.
   */
  export function cstring_ptr(handle: number): number;
  /**
   * Releases the CString handle.
   */
  export function cstring_free(handle: number): void;
  /**
   * Builds an OsString from a UTF-8 source and returns a handle.
   */
  export function osstr_from_str(s: string): number;
  /**
   * Converts the OsString handle to a UTF-8 string handle. 0 if not valid UTF-8.
   */
  export function osstr_to_str(handle: number): number;
  /**
   * Releases the OsString handle.
   */
  export function osstr_free(handle: number): void;
  const _default: {
    cstr_from_ptr: (typeof import("rts"))["ffi"]["cstr_from_ptr"];
    cstr_len: (typeof import("rts"))["ffi"]["cstr_len"];
    cstr_to_str: (typeof import("rts"))["ffi"]["cstr_to_str"];
    cstring_new: (typeof import("rts"))["ffi"]["cstring_new"];
    cstring_ptr: (typeof import("rts"))["ffi"]["cstring_ptr"];
    cstring_free: (typeof import("rts"))["ffi"]["cstring_free"];
    osstr_from_str: (typeof import("rts"))["ffi"]["osstr_from_str"];
    osstr_to_str: (typeof import("rts"))["ffi"]["osstr_to_str"];
    osstr_free: (typeof import("rts"))["ffi"]["osstr_free"];
  };
  export default _default;
}

declare module "rts:atomic" {
  /**
   * Primitivas atomicas (AtomicI64, AtomicBool, fences) baseadas em std::sync::atomic.
   */
  /**
   * Aloca um AtomicI64 inicializado com `value` e retorna o handle.
   */
  export function i64_new(value: number): number;
  /**
   * Le o valor atual do AtomicI64 (SeqCst). 0 se handle invalido.
   */
  export function i64_load(handle: number): number;
  /**
   * Escreve `value` no AtomicI64 (SeqCst). No-op se handle invalido.
   */
  export function i64_store(handle: number, value: number): void;
  /**
   * Soma `delta` e retorna o valor anterior. 0 se handle invalido.
   */
  export function i64_fetch_add(handle: number, delta: number): number;
  /**
   * Subtrai `delta` e retorna o valor anterior. 0 se handle invalido.
   */
  export function i64_fetch_sub(handle: number, delta: number): number;
  /**
   * AND bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
   */
  export function i64_fetch_and(handle: number, mask: number): number;
  /**
   * OR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
   */
  export function i64_fetch_or(handle: number, mask: number): number;
  /**
   * XOR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
   */
  export function i64_fetch_xor(handle: number, mask: number): number;
  /**
   * Troca o valor por `value` e retorna o valor anterior. 0 se handle invalido.
   */
  export function i64_swap(handle: number, value: number): number;
  /**
   * Compare-and-swap. Se valor atual == `expected`, escreve `new`. Retorna o valor anterior.
   */
  export function i64_cas(handle: number, expected: number, new_value: number): number;
  /**
   * Aloca um AtomicBool inicializado com `value` e retorna o handle.
   */
  export function bool_new(value: boolean): number;
  /**
   * Le o valor atual do AtomicBool (SeqCst). false se handle invalido.
   */
  export function bool_load(handle: number): boolean;
  /**
   * Escreve `value` no AtomicBool (SeqCst). No-op se handle invalido.
   */
  export function bool_store(handle: number, value: boolean): void;
  /**
   * Troca o valor por `value` e retorna o valor anterior. false se handle invalido.
   */
  export function bool_swap(handle: number, value: boolean): boolean;
  /**
   * Aloca um AtomicF64 inicializado com `value` e retorna o handle.
   */
  export function f64_new(value: number): number;
  /**
   * Le o valor atual do AtomicF64 (SeqCst). 0.0 se handle invalido.
   */
  export function f64_load(handle: number): number;
  /**
   * Escreve `value` no AtomicF64 (SeqCst). No-op se handle invalido.
   */
  export function f64_store(handle: number, value: number): void;
  /**
   * Soma `delta` e retorna o valor anterior (loop CAS internamente). 0.0 se handle invalido.
   */
  export function f64_fetch_add(handle: number, delta: number): number;
  /**
   * Troca o valor por `value` e retorna o valor anterior. 0.0 se handle invalido.
   */
  export function f64_swap(handle: number, value: number): number;
  /**
   * Memory fence Acquire.
   */
  export function fence_acquire(): void;
  /**
   * Memory fence Release.
   */
  export function fence_release(): void;
  /**
   * Memory fence SeqCst.
   */
  export function fence_seq_cst(): void;
  const _default: {
    i64_new: (typeof import("rts"))["atomic"]["i64_new"];
    i64_load: (typeof import("rts"))["atomic"]["i64_load"];
    i64_store: (typeof import("rts"))["atomic"]["i64_store"];
    i64_fetch_add: (typeof import("rts"))["atomic"]["i64_fetch_add"];
    i64_fetch_sub: (typeof import("rts"))["atomic"]["i64_fetch_sub"];
    i64_fetch_and: (typeof import("rts"))["atomic"]["i64_fetch_and"];
    i64_fetch_or: (typeof import("rts"))["atomic"]["i64_fetch_or"];
    i64_fetch_xor: (typeof import("rts"))["atomic"]["i64_fetch_xor"];
    i64_swap: (typeof import("rts"))["atomic"]["i64_swap"];
    i64_cas: (typeof import("rts"))["atomic"]["i64_cas"];
    bool_new: (typeof import("rts"))["atomic"]["bool_new"];
    bool_load: (typeof import("rts"))["atomic"]["bool_load"];
    bool_store: (typeof import("rts"))["atomic"]["bool_store"];
    bool_swap: (typeof import("rts"))["atomic"]["bool_swap"];
    f64_new: (typeof import("rts"))["atomic"]["f64_new"];
    f64_load: (typeof import("rts"))["atomic"]["f64_load"];
    f64_store: (typeof import("rts"))["atomic"]["f64_store"];
    f64_fetch_add: (typeof import("rts"))["atomic"]["f64_fetch_add"];
    f64_swap: (typeof import("rts"))["atomic"]["f64_swap"];
    fence_acquire: (typeof import("rts"))["atomic"]["fence_acquire"];
    fence_release: (typeof import("rts"))["atomic"]["fence_release"];
    fence_seq_cst: (typeof import("rts"))["atomic"]["fence_seq_cst"];
  };
  export default _default;
}

declare module "rts:sync" {
  /**
   * Primitivas de sincronizacao (Mutex, RwLock, OnceLock) baseadas em std::sync.
   */
  /**
   * Aloca um Mutex protegendo um valor i64 inicializado com `initial`.
   */
  export function mutex_new(initial: number): number;
  /**
   * Bloqueia ate adquirir o Mutex e retorna o valor interno protegido. 0 se handle invalido.
   */
  export function mutex_lock(mutex: number): number;
  /**
   * Tenta adquirir o Mutex sem bloquear. Retorna o valor interno em caso de sucesso, 0 se ja estava lockado ou handle invalido.
   */
  export function mutex_try_lock(mutex: number): number;
  /**
   * Escreve `value` no Mutex. Caller deve ter chamado lock/try_lock antes (responsabilidade do caller).
   */
  export function mutex_set(mutex: number, value: number): void;
  /**
   * Libera o Mutex previamente adquirido por lock/try_lock. No-op se nao havia guard ativo.
   */
  export function mutex_unlock(mutex: number): void;
  /**
   * Libera o Mutex e seu slot na HandleTable.
   */
  export function mutex_free(mutex: number): void;
  /**
   * Aloca um RwLock protegendo um valor i64 inicializado com `initial`.
   */
  export function rwlock_new(initial: number): number;
  /**
   * Adquire um read guard (compartilhado) e retorna um handle de guard. Liberar via rwlock_unlock(guard). 0 se handle invalido.
   */
  export function rwlock_read(rwlock: number): number;
  /**
   * Adquire um write guard (exclusivo) e retorna um handle de guard. Liberar via rwlock_unlock(guard). 0 se handle invalido.
   */
  export function rwlock_write(rwlock: number): number;
  /**
   * Libera um guard previamente adquirido via rwlock_read/rwlock_write.
   */
  export function rwlock_unlock(guard: number): void;
  /**
   * Aloca um OnceLock e retorna o handle.
   */
  export function once_new(): number;
  /**
   * Executa `fn_ptr` (ponteiro para `extern "C" fn()`) exatamente uma vez por OnceLock. Chamadas subsequentes sao no-op.
   */
  export function once_call(once: number, fn_ptr: number): void;
  const _default: {
    mutex_new: (typeof import("rts"))["sync"]["mutex_new"];
    mutex_lock: (typeof import("rts"))["sync"]["mutex_lock"];
    mutex_try_lock: (typeof import("rts"))["sync"]["mutex_try_lock"];
    mutex_set: (typeof import("rts"))["sync"]["mutex_set"];
    mutex_unlock: (typeof import("rts"))["sync"]["mutex_unlock"];
    mutex_free: (typeof import("rts"))["sync"]["mutex_free"];
    rwlock_new: (typeof import("rts"))["sync"]["rwlock_new"];
    rwlock_read: (typeof import("rts"))["sync"]["rwlock_read"];
    rwlock_write: (typeof import("rts"))["sync"]["rwlock_write"];
    rwlock_unlock: (typeof import("rts"))["sync"]["rwlock_unlock"];
    once_new: (typeof import("rts"))["sync"]["once_new"];
    once_call: (typeof import("rts"))["sync"]["once_call"];
  };
  export default _default;
}

declare module "rts:string" {
  /**
   * Rich string operations beyond the basic gc pool.
   */
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
  const _default: {
    contains: (typeof import("rts"))["string"]["contains"];
    starts_with: (typeof import("rts"))["string"]["starts_with"];
    ends_with: (typeof import("rts"))["string"]["ends_with"];
    find: (typeof import("rts"))["string"]["find"];
    to_upper: (typeof import("rts"))["string"]["to_upper"];
    to_lower: (typeof import("rts"))["string"]["to_lower"];
    trim: (typeof import("rts"))["string"]["trim"];
    trim_start: (typeof import("rts"))["string"]["trim_start"];
    trim_end: (typeof import("rts"))["string"]["trim_end"];
    repeat: (typeof import("rts"))["string"]["repeat"];
    replace: (typeof import("rts"))["string"]["replace"];
    replacen: (typeof import("rts"))["string"]["replacen"];
    char_count: (typeof import("rts"))["string"]["char_count"];
    byte_len: (typeof import("rts"))["string"]["byte_len"];
    char_at: (typeof import("rts"))["string"]["char_at"];
    char_code_at: (typeof import("rts"))["string"]["char_code_at"];
  };
  export default _default;
}

declare module "rts:process" {
  /**
   * Process control: exit/abort, pid, spawn/wait/kill children.
   */
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
  const _default: {
    exit: (typeof import("rts"))["process"]["exit"];
    abort: (typeof import("rts"))["process"]["abort"];
    pid: (typeof import("rts"))["process"]["pid"];
    args_count: (typeof import("rts"))["process"]["args_count"];
    arg_at: (typeof import("rts"))["process"]["arg_at"];
    spawn: (typeof import("rts"))["process"]["spawn"];
    wait: (typeof import("rts"))["process"]["wait"];
    kill: (typeof import("rts"))["process"]["kill"];
  };
  export default _default;
}

declare module "rts:ptr" {
  /**
   * Operacoes raw sobre ponteiros (std::ptr). UNSAFE — caller verifica validez.
   */
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
  const _default: {
    null: (typeof import("rts"))["ptr"]["null"];
    is_null: (typeof import("rts"))["ptr"]["is_null"];
    read_i64: (typeof import("rts"))["ptr"]["read_i64"];
    read_i32: (typeof import("rts"))["ptr"]["read_i32"];
    read_u8: (typeof import("rts"))["ptr"]["read_u8"];
    read_f64: (typeof import("rts"))["ptr"]["read_f64"];
    write_i64: (typeof import("rts"))["ptr"]["write_i64"];
    write_i32: (typeof import("rts"))["ptr"]["write_i32"];
    write_u8: (typeof import("rts"))["ptr"]["write_u8"];
    write_f64: (typeof import("rts"))["ptr"]["write_f64"];
    copy: (typeof import("rts"))["ptr"]["copy"];
    copy_nonoverlapping: (typeof import("rts"))["ptr"]["copy_nonoverlapping"];
    write_bytes: (typeof import("rts"))["ptr"]["write_bytes"];
    offset: (typeof import("rts"))["ptr"]["offset"];
  };
  export default _default;
}

declare module "rts:os" {
  /**
   * OS and environment info: platform, arch, special directories.
   */
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
  const _default: {
    platform: (typeof import("rts"))["os"]["platform"];
    arch: (typeof import("rts"))["os"]["arch"];
    family: (typeof import("rts"))["os"]["family"];
    eol: (typeof import("rts"))["os"]["eol"];
    home_dir: (typeof import("rts"))["os"]["home_dir"];
    temp_dir: (typeof import("rts"))["os"]["temp_dir"];
    config_dir: (typeof import("rts"))["os"]["config_dir"];
    cache_dir: (typeof import("rts"))["os"]["cache_dir"];
  };
  export default _default;
}

declare module "rts:collections" {
  /**
   * Handle-based HashMap and Vec backed by std::collections.
   */
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
  const _default: {
    map_new: (typeof import("rts"))["collections"]["map_new"];
    map_free: (typeof import("rts"))["collections"]["map_free"];
    map_len: (typeof import("rts"))["collections"]["map_len"];
    map_has: (typeof import("rts"))["collections"]["map_has"];
    map_get: (typeof import("rts"))["collections"]["map_get"];
    map_set: (typeof import("rts"))["collections"]["map_set"];
    map_delete: (typeof import("rts"))["collections"]["map_delete"];
    map_clear: (typeof import("rts"))["collections"]["map_clear"];
    map_key_at: (typeof import("rts"))["collections"]["map_key_at"];
    vec_new: (typeof import("rts"))["collections"]["vec_new"];
    vec_free: (typeof import("rts"))["collections"]["vec_free"];
    vec_len: (typeof import("rts"))["collections"]["vec_len"];
    vec_push: (typeof import("rts"))["collections"]["vec_push"];
    vec_pop: (typeof import("rts"))["collections"]["vec_pop"];
    vec_get: (typeof import("rts"))["collections"]["vec_get"];
    vec_set: (typeof import("rts"))["collections"]["vec_set"];
    vec_clear: (typeof import("rts"))["collections"]["vec_clear"];
  };
  export default _default;
}

declare module "rts:hash" {
  /**
   * Non-cryptographic hashing via std::hash::DefaultHasher (SipHash-1-3).
   */
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
  const _default: {
    hash_str: (typeof import("rts"))["hash"]["hash_str"];
    hash_bytes: (typeof import("rts"))["hash"]["hash_bytes"];
    hash_i64: (typeof import("rts"))["hash"]["hash_i64"];
    hash_combine: (typeof import("rts"))["hash"]["hash_combine"];
  };
  export default _default;
}

declare module "rts:hint" {
  /**
   * Performance hints (std::hint): spin_loop, black_box, unreachable, assert_unchecked.
   */
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
  const _default: {
    spin_loop: (typeof import("rts"))["hint"]["spin_loop"];
    black_box_i64: (typeof import("rts"))["hint"]["black_box_i64"];
    black_box_f64: (typeof import("rts"))["hint"]["black_box_f64"];
    unreachable: (typeof import("rts"))["hint"]["unreachable"];
    assert_unchecked: (typeof import("rts"))["hint"]["assert_unchecked"];
  };
  export default _default;
}

declare module "rts:fmt" {
  /**
   * Parse and format primitives (string <-> number).
   */
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
  const _default: {
    parse_i64: (typeof import("rts"))["fmt"]["parse_i64"];
    parse_f64: (typeof import("rts"))["fmt"]["parse_f64"];
    parse_bool: (typeof import("rts"))["fmt"]["parse_bool"];
    fmt_i64: (typeof import("rts"))["fmt"]["fmt_i64"];
    fmt_f64: (typeof import("rts"))["fmt"]["fmt_f64"];
    fmt_bool: (typeof import("rts"))["fmt"]["fmt_bool"];
    fmt_hex: (typeof import("rts"))["fmt"]["fmt_hex"];
    fmt_bin: (typeof import("rts"))["fmt"]["fmt_bin"];
    fmt_oct: (typeof import("rts"))["fmt"]["fmt_oct"];
    fmt_f64_prec: (typeof import("rts"))["fmt"]["fmt_f64_prec"];
  };
  export default _default;
}

declare module "rts:crypto" {
  /**
   * Cryptographic primitives: SHA-256, CSPRNG, hex, base64.
   */
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
  const _default: {
    random_bytes: (typeof import("rts"))["crypto"]["random_bytes"];
    random_i64: (typeof import("rts"))["crypto"]["random_i64"];
    random_buffer: (typeof import("rts"))["crypto"]["random_buffer"];
    sha256_str: (typeof import("rts"))["crypto"]["sha256_str"];
    sha256_bytes: (typeof import("rts"))["crypto"]["sha256_bytes"];
    hex_encode: (typeof import("rts"))["crypto"]["hex_encode"];
    hex_decode: (typeof import("rts"))["crypto"]["hex_decode"];
    base64_encode: (typeof import("rts"))["crypto"]["base64_encode"];
    base64_decode: (typeof import("rts"))["crypto"]["base64_decode"];
  };
  export default _default;
}

declare module "rts:regex" {
  /**
   * Expressoes regulares via crate `regex` (sintaxe RE2-like, sem backreferences).
   */
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
  const _default: {
    compile: (typeof import("rts"))["regex"]["compile"];
    free: (typeof import("rts"))["regex"]["free"];
    test: (typeof import("rts"))["regex"]["test"];
    find: (typeof import("rts"))["regex"]["find"];
    find_at: (typeof import("rts"))["regex"]["find_at"];
    replace: (typeof import("rts"))["regex"]["replace"];
    replace_all: (typeof import("rts"))["regex"]["replace_all"];
    match_count: (typeof import("rts"))["regex"]["match_count"];
  };
  export default _default;
}

declare module "rts:ui" {
  /**
   * FLTK GUI: windows, widgets, menus, text, drawing, and dialogs.
   */
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
   * Shows an input dialog (label, def). Returns GC string handle, or 0 on cancel.
   */
  export function dialog_input(label: string, def: string): number;
  const _default: {
    app_new: (typeof import("rts"))["ui"]["app_new"];
    app_run: (typeof import("rts"))["ui"]["app_run"];
    app_free: (typeof import("rts"))["ui"]["app_free"];
    window_new: (typeof import("rts"))["ui"]["window_new"];
    window_show: (typeof import("rts"))["ui"]["window_show"];
    window_end: (typeof import("rts"))["ui"]["window_end"];
    window_free: (typeof import("rts"))["ui"]["window_free"];
    window_set_callback: (typeof import("rts"))["ui"]["window_set_callback"];
    window_set_color: (typeof import("rts"))["ui"]["window_set_color"];
    window_resize: (typeof import("rts"))["ui"]["window_resize"];
    widget_set_label: (typeof import("rts"))["ui"]["widget_set_label"];
    widget_label: (typeof import("rts"))["ui"]["widget_label"];
    widget_set_callback: (typeof import("rts"))["ui"]["widget_set_callback"];
    widget_set_callback_with_ud: (typeof import("rts"))["ui"]["widget_set_callback_with_ud"];
    widget_set_color: (typeof import("rts"))["ui"]["widget_set_color"];
    widget_set_label_color: (typeof import("rts"))["ui"]["widget_set_label_color"];
    widget_resize: (typeof import("rts"))["ui"]["widget_resize"];
    widget_redraw: (typeof import("rts"))["ui"]["widget_redraw"];
    widget_hide: (typeof import("rts"))["ui"]["widget_hide"];
    widget_show: (typeof import("rts"))["ui"]["widget_show"];
    widget_set_draw: (typeof import("rts"))["ui"]["widget_set_draw"];
    button_new: (typeof import("rts"))["ui"]["button_new"];
    frame_new: (typeof import("rts"))["ui"]["frame_new"];
    check_new: (typeof import("rts"))["ui"]["check_new"];
    check_value: (typeof import("rts"))["ui"]["check_value"];
    check_set_value: (typeof import("rts"))["ui"]["check_set_value"];
    radio_new: (typeof import("rts"))["ui"]["radio_new"];
    radio_value: (typeof import("rts"))["ui"]["radio_value"];
    radio_set_value: (typeof import("rts"))["ui"]["radio_set_value"];
    input_new: (typeof import("rts"))["ui"]["input_new"];
    input_value: (typeof import("rts"))["ui"]["input_value"];
    input_set_value: (typeof import("rts"))["ui"]["input_set_value"];
    output_new: (typeof import("rts"))["ui"]["output_new"];
    output_set_value: (typeof import("rts"))["ui"]["output_set_value"];
    slider_new: (typeof import("rts"))["ui"]["slider_new"];
    slider_value: (typeof import("rts"))["ui"]["slider_value"];
    slider_set_value: (typeof import("rts"))["ui"]["slider_set_value"];
    slider_set_bounds: (typeof import("rts"))["ui"]["slider_set_bounds"];
    progress_new: (typeof import("rts"))["ui"]["progress_new"];
    progress_value: (typeof import("rts"))["ui"]["progress_value"];
    progress_set_value: (typeof import("rts"))["ui"]["progress_set_value"];
    spinner_new: (typeof import("rts"))["ui"]["spinner_new"];
    spinner_value: (typeof import("rts"))["ui"]["spinner_value"];
    spinner_set_value: (typeof import("rts"))["ui"]["spinner_set_value"];
    spinner_set_bounds: (typeof import("rts"))["ui"]["spinner_set_bounds"];
    menubar_new: (typeof import("rts"))["ui"]["menubar_new"];
    menubar_add: (typeof import("rts"))["ui"]["menubar_add"];
    menubar_free: (typeof import("rts"))["ui"]["menubar_free"];
    textbuf_new: (typeof import("rts"))["ui"]["textbuf_new"];
    textbuf_set_text: (typeof import("rts"))["ui"]["textbuf_set_text"];
    textbuf_text: (typeof import("rts"))["ui"]["textbuf_text"];
    textbuf_append: (typeof import("rts"))["ui"]["textbuf_append"];
    textbuf_free: (typeof import("rts"))["ui"]["textbuf_free"];
    textdisplay_new: (typeof import("rts"))["ui"]["textdisplay_new"];
    textdisplay_set_buffer: (typeof import("rts"))["ui"]["textdisplay_set_buffer"];
    texteditor_new: (typeof import("rts"))["ui"]["texteditor_new"];
    texteditor_set_buffer: (typeof import("rts"))["ui"]["texteditor_set_buffer"];
    draw_rect: (typeof import("rts"))["ui"]["draw_rect"];
    draw_rect_fill: (typeof import("rts"))["ui"]["draw_rect_fill"];
    draw_line: (typeof import("rts"))["ui"]["draw_line"];
    draw_circle: (typeof import("rts"))["ui"]["draw_circle"];
    draw_arc: (typeof import("rts"))["ui"]["draw_arc"];
    draw_text: (typeof import("rts"))["ui"]["draw_text"];
    set_draw_color: (typeof import("rts"))["ui"]["set_draw_color"];
    set_font: (typeof import("rts"))["ui"]["set_font"];
    set_line_style: (typeof import("rts"))["ui"]["set_line_style"];
    measure_width: (typeof import("rts"))["ui"]["measure_width"];
    alert: (typeof import("rts"))["ui"]["alert"];
    dialog_ask: (typeof import("rts"))["ui"]["dialog_ask"];
    dialog_input: (typeof import("rts"))["ui"]["dialog_input"];
  };
  export default _default;
}

declare module "rts:runtime" {
  /**
   * Dynamic TS/JS evaluation. JIT path uses inline compilation; AOT path spawns rts.
   */
  /**
   * Evaluates a TS/JS source string. Returns the program exit code.
   */
  export function eval(src: string): number;
  /**
   * Loads and evaluates a TS/JS file at the given path. Returns the program exit code.
   */
  export function eval_file(path: string): number;
  const _default: {
    eval: (typeof import("rts"))["runtime"]["eval"];
    eval_file: (typeof import("rts"))["runtime"]["eval_file"];
  };
  export default _default;
}

declare module "rts:test_core" {
  /**
   * Low-level test runner primitives. Use rts:test for the high-level API.
   */
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
  const _default: {
    suite_begin: (typeof import("rts"))["test_core"]["suite_begin"];
    suite_end: (typeof import("rts"))["test_core"]["suite_end"];
    case_begin: (typeof import("rts"))["test_core"]["case_begin"];
    case_end: (typeof import("rts"))["test_core"]["case_end"];
    case_fail: (typeof import("rts"))["test_core"]["case_fail"];
    case_fail_diff: (typeof import("rts"))["test_core"]["case_fail_diff"];
    print_summary: (typeof import("rts"))["test_core"]["print_summary"];
  };
  export default _default;
}

declare module "rts:thread" {
  /**
   * Primitivas de threads (spawn/join/detach/scope/id/sleep) baseadas em std::thread.
   */
  /**
   * Cria uma nova thread executando `fn_ptr(arg)`. `fn_ptr` e um ponteiro para `extern "C" fn(u64) -> u64`. Retorna handle do JoinHandle, 0 em falha.
   */
  export function spawn(fn_ptr: number, arg: number): number;
  /**
   * Variante de `spawn` que passa um `userdata` (ex: handle de `this`) como primeiro argumento da fn. `fn_ptr` aponta para `extern "C" fn(u64, u64) -> u64` (ud, arg).
   */
  export function spawn_with_ud(fn_ptr: number, arg: number, userdata: number): number;
  /**
   * Roda `body()` num escopo que aguarda automaticamente todas as threads spawnadas durante sua execucao. Analogo a `std::thread::scope`.
   */
  export function scope(body: () => void): void;
  /**
   * Variante com userdata para `thread.scope` quando o body captura `this`.
   */
  export function scope_with_ud(body: number, userdata: number): void;
  /**
   * Aguarda a thread terminar e retorna o valor retornado por ela. Consome o handle. 0 se handle invalido ou a thread fez panic.
   */
  export function join(thread: number): number;
  /**
   * Libera o JoinHandle sem aguardar. A thread continua rodando ate completar.
   */
  export function detach(thread: number): void;
  /**
   * Id da thread atual (estavel por thread, atribuido na primeira chamada). Sempre != 0.
   */
  export function id(): number;
  /**
   * Pausa a thread atual por `ms` milissegundos. Valores negativos sao tratados como 0.
   */
  export function sleep_ms(ms: number): void;
  const _default: {
    spawn: (typeof import("rts"))["thread"]["spawn"];
    spawn_with_ud: (typeof import("rts"))["thread"]["spawn_with_ud"];
    scope: (typeof import("rts"))["thread"]["scope"];
    scope_with_ud: (typeof import("rts"))["thread"]["scope_with_ud"];
    join: (typeof import("rts"))["thread"]["join"];
    detach: (typeof import("rts"))["thread"]["detach"];
    id: (typeof import("rts"))["thread"]["id"];
    sleep_ms: (typeof import("rts"))["thread"]["sleep_ms"];
  };
  export default _default;
}

declare module "rts:parallel" {
  /**
   * Paralelismo de dados via Rayon (map/for_each/reduce sobre Vec<i64>).
   */
  /**
   * Aplica `fn_ptr(x)` em paralelo sobre cada elemento do Vec<i64> `vec_handle`. Retorna novo Vec<i64> com os resultados. `fn_ptr` e `extern "C" fn(i64) -> i64`.
   */
  export function map(vec_handle: number, fn_ptr: number): number;
  /**
   * Executa `fn_ptr(x)` em paralelo para cada elemento do Vec<i64> `vec_handle`. `fn_ptr` e `extern "C" fn(i64)`.
   */
  export function for_each(vec_handle: number, fn_ptr: number): void;
  /**
   * Reduz Vec<i64> `vec_handle` com `fn_ptr(acc, x) -> acc` em paralelo (divide-e-conquista). `identity` e o elemento neutro da operacao (0 para soma, 1 para produto). `fn_ptr` deve ser associativo.
   */
  export function reduce(vec_handle: number, identity: number, fn_ptr: number): number;
  /**
   * Retorna o numero de threads no pool Rayon global.
   */
  export function num_threads(): number;
  const _default: {
    map: (typeof import("rts"))["parallel"]["map"];
    for_each: (typeof import("rts"))["parallel"]["for_each"];
    reduce: (typeof import("rts"))["parallel"]["reduce"];
    num_threads: (typeof import("rts"))["parallel"]["num_threads"];
  };
  export default _default;
}

declare module "rts:tls" {
  /**
   * TLS 1.2/1.3 client sync via rustls (HTTPS support).
   */
  /**
   * Wraps tcp_handle numa conexao TLS client. Consome o tcp_handle. SNI = sni_hostname. Retorna stream handle ou 0 (handshake falhou).
   */
  export function client(tcp: number, sniHostname: string): number;
  /**
   * Envia bytes encriptados pelo TLS stream. Retorna bytes plain enviados ou -1.
   */
  export function send(stream: number, data: string): number;
  /**
   * Le ate `len` bytes plain do TLS stream. Retorna bytes lidos (0 = EOF, -1 = erro).
   */
  export function recv(stream: number, bufPtr: number, len: number): number;
  /**
   * Fecha o stream TLS (close_notify) e libera o handle.
   */
  export function close(stream: number): void;
  const _default: {
    client: (typeof import("rts"))["tls"]["client"];
    send: (typeof import("rts"))["tls"]["send"];
    recv: (typeof import("rts"))["tls"]["recv"];
    close: (typeof import("rts"))["tls"]["close"];
  };
  export default _default;
}
