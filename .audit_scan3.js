// Pass 3: any local fn that TAKES a &Context / &mut Context yet itself calls an
// AMBIENT (borrow-taking) core entry point, or opens a new borrow. Such a fn is
// unusable under the borrow it is written to be used under.
const fs = require('fs');
const path = require('path');
const ROOTS = ['crates/rts-node-rwk/src', 'crates/rts-std-rwk/src'];

const AMBIENT_CORE = new Set([
  'undefined_value','null_value','text_of','make_array','is_array','described','deep_copy',
  'drain_microtasks','evaluate','evaluator','call','own_keys','strict_equals','to_boolean',
  'get_indexed','set_indexed','set_prototype','get_prototype','object_new','get_property',
  'set_property','array_new','iterate','array_append','array_append_all','global_get','global_set',
  'has_property','delete_property','construct','call_with_args','construct_with_args',
  'module_binding','module_namespace','throw','type_of','string_const','template_strings',
  'number_to_string','closure_new','define_getter','define_setter','pump_sources',
  'bigint_new','negate','regex_new','mark_derived','super_construct','rest_arguments',
  'with_runtime','with_current',
]);

function walk(dir, out) { for (const e of fs.readdirSync(dir,{withFileTypes:true})) { const p=path.join(dir,e.name); if (e.isDirectory()) walk(p,out); else if (e.name.endsWith('.rs')) out.push(p); } return out; }
function blank(src) {
  const a=src.split(''); let i=0; const n=src.length;
  const kill=(f,t)=>{for(let k=f;k<t;k++) if(a[k]!=='\n') a[k]=' ';};
  while(i<n){ const c=src[i];
    if(c==='/'&&src[i+1]==='/'){let j=i;while(j<n&&src[j]!=='\n')j++;kill(i,j);i=j;continue;}
    if(c==='/'&&src[i+1]==='*'){let d=1,j=i+2;while(j<n&&d>0){if(src[j]==='/'&&src[j+1]==='*'){d++;j+=2;}else if(src[j]==='*'&&src[j+1]==='/'){d--;j+=2;}else j++;}kill(i,j);i=j;continue;}
    if(c==='r'&&(src[i+1]==='"'||src[i+1]==='#')){let j=i+1,h=0;while(src[j]==='#'){h++;j++;}if(src[j]==='"'){j++;const t='"'+'#'.repeat(h);const e=src.indexOf(t,j);const s=e===-1?n:e+t.length;kill(i,s);i=s;continue;}}
    if(c==='"'){let j=i+1;while(j<n){if(src[j]==='\\')j+=2;else if(src[j]==='"'){j++;break;}else j++;}kill(i,j);i=j;continue;}
    i++; }
  return a.join('');
}
function lineOf(src,off){let l=1;for(let i=0;i<off;i++)if(src[i]==='\n')l++;return l;}

const files=[]; for (const r of ROOTS) walk(r, files);
const out=[];
for (const file of files) {
  const src=fs.readFileSync(file,'utf8'); const clean=blank(src);
  const re=/\bfn\s+([a-zA-Z_][a-zA-Z_0-9]*)\s*(<[^>]*>)?\s*\(/g; let m;
  while((m=re.exec(clean))!==null){
    const open=m.index+m[0].length-1;
    let d=0,j=open,close=-1;
    for(;j<clean.length;j++){const ch=clean[j]; if(ch==='(')d++; else if(ch===')'){d--; if(d===0){close=j;break;}}}
    if(close===-1) continue;
    const params=clean.slice(open+1,close);
    if(!/&\s*(mut\s+)?(entry::)?Context\b/.test(params)) continue;
    let k=close+1; while(k<clean.length&&clean[k]!=='{'&&clean[k]!==';')k++;
    if(clean[k]!=='{') continue;
    let bd=0,e=k; for(;e<clean.length;e++){const ch=clean[e]; if(ch==='{')bd++; else if(ch==='}'){bd--; if(bd===0)break;}}
    const body=clean.slice(k+1,e);
    const idRe=/(^|[^.\w])([a-zA-Z_][a-zA-Z_0-9]*)\s*\(/g; let q;
    while((q=idRe.exec(body))!==null){
      const name=q[2];
      if(!AMBIENT_CORE.has(name)) continue;
      const abs=k+1+q.index+q[1].length;
      const ln=lineOf(src,abs);
      out.push(`${file.replace(/\\/g,'/')}:${ln}  fn ${m[1]}(&Context) at line ${lineOf(src,m.index)} calls AMBIENT ${name}  ::  ${src.split('\n')[ln-1].trim()}`);
    }
  }
}
out.forEach(l=>console.log(l));
console.log('TOTAL', out.length);
