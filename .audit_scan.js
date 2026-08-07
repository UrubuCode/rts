// Read-only audit: find ambient entry:: calls inside a with_runtime/with_current borrow.
const fs = require('fs');
const path = require('path');

const ROOTS = [
  'crates/rts-node-rwk/src',
  'crates/rts-std-rwk/src',
];

// Ambient = the function itself takes the thread-local borrow.
const AMBIENT = new Set([
  'undefined_value','null_value','text_of','make_array','is_array','described','deep_copy',
  'drain_microtasks','evaluate','evaluator','call','own_keys','strict_equals','to_boolean',
  'get_indexed','set_indexed','set_prototype','get_prototype','object_new','get_property',
  'set_property','array_new','iterate','array_append','array_append_all','global_get','global_set',
  'has_property','delete_property','instance_of','construct','call_with_args','construct_with_args',
  'module_binding','module_namespace','throw','type_of','string_const','template_strings',
  'add','number_to_string','closure_new','define_getter','define_setter','pump_sources',
  'bigint_new','negate','regex_new','mark_derived','super_construct','rest_arguments',
  'cache_resolve','cache_resolve_store','alloc','write_barrier','with_runtime','with_current',
  'divide','greater','greater_equal','less','less_equal','loose_equals','multiply','remainder','subtract',
  'bit_and','bit_not','bit_or','bit_xor','exponent','shift_left','shift_right','shift_right_unsigned',
]);

function walk(dir, out) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith('.rs')) out.push(p);
  }
  return out;
}

// Blank out comments and string/char literals, preserving length + newlines.
function blank(src) {
  const a = src.split('');
  let i = 0;
  const n = src.length;
  const kill = (from, to) => { for (let k = from; k < to; k++) if (a[k] !== '\n') a[k] = ' '; };
  while (i < n) {
    const c = src[i];
    if (c === '/' && src[i + 1] === '/') {
      let j = i; while (j < n && src[j] !== '\n') j++;
      kill(i, j); i = j; continue;
    }
    if (c === '/' && src[i + 1] === '*') {
      let depth = 1, j = i + 2;
      while (j < n && depth > 0) {
        if (src[j] === '/' && src[j + 1] === '*') { depth++; j += 2; }
        else if (src[j] === '*' && src[j + 1] === '/') { depth--; j += 2; }
        else j++;
      }
      kill(i, j); i = j; continue;
    }
    if (c === 'r' && (src[i + 1] === '"' || src[i + 1] === '#')) {
      let j = i + 1, hashes = 0;
      while (src[j] === '#') { hashes++; j++; }
      if (src[j] === '"') {
        j++;
        const term = '"' + '#'.repeat(hashes);
        const end = src.indexOf(term, j);
        const stop = end === -1 ? n : end + term.length;
        kill(i, stop); i = stop; continue;
      }
    }
    if (c === '"') {
      let j = i + 1;
      while (j < n) { if (src[j] === '\\') j += 2; else if (src[j] === '"') { j++; break; } else j++; }
      kill(i, j); i = j; continue;
    }
    i++;
  }
  return a.join('');
}

function lineOf(src, off) { let l = 1; for (let i = 0; i < off; i++) if (src[i] === '\n') l++; return l; }

const results = [];
for (const root of ROOTS) {
  for (const file of walk(root, [])) {
    const src = fs.readFileSync(file, 'utf8');
    const clean = blank(src);
    const re = /\bwith_(runtime|current)\s*\(/g;
    let m;
    while ((m = re.exec(clean)) !== null) {
      const openParen = m.index + m[0].length - 1;
      // match parens to find the end of the call
      let depth = 0, j = openParen, end = -1;
      for (; j < clean.length; j++) {
        const ch = clean[j];
        if (ch === '(') depth++;
        else if (ch === ')') { depth--; if (depth === 0) { end = j; break; } }
      }
      if (end === -1) end = clean.length;
      const body = clean.slice(openParen + 1, end);
      const idRe = /\b([a-z_][a-z_0-9]*)\s*\(/g;
      let k;
      while ((k = idRe.exec(body)) !== null) {
        const name = k[1];
        if (!AMBIENT.has(name)) continue;
        const abs = openParen + 1 + k.index;
        results.push({
          file: file.replace(/\\/g, '/'),
          line: lineOf(src, abs),
          name,
          outerLine: lineOf(src, m.index),
          text: src.split('\n')[lineOf(src, abs) - 1].trim(),
        });
      }
    }
  }
}
results.sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line);
for (const r of results) {
  console.log(`${r.file}:${r.line} [${r.name}] (borrow opened at line ${r.outerLine})  ${r.text}`);
}
console.log('TOTAL', results.length);
