/// WGSL: vertex = pos+normal (slot 0) × instância model(4×vec4)+color (slot 1).
/// Uniform (group 0): viewProj + luz (xyz=dir, w=ambiente). Shading difuso.
pub(in crate::frame::scene3d) const SHADER: &str = r#"
struct Cam {
  view_proj: mat4x4<f32>,
  light: vec4<f32>,
  cam_pos: vec4<f32>,
  cam_right: vec4<f32>,  // xyz = right,   w = tanH
  cam_up: vec4<f32>,     // xyz = up,      w = tanV
  cam_fwd: vec4<f32>,    // xyz = forward
  light_vp: mat4x4<f32>, // view·proj da LUZ (shadow map)
  water: vec4<f32>,      // x = escala da partícula de água (render instanciado)
};
@group(0) @binding(0) var<uniform> cam: Cam;
// shadow map (group 1): depth da cena vista da luz + comparison sampler
@group(1) @binding(0) var shadow_tex: texture_depth_2d;
@group(1) @binding(1) var shadow_samp: sampler_comparison;
// textura de ALBEDO real (group 2): imagem decodificada + sampler linear/repeat.
// Bindada por-draw; quando o objeto não tem textura, uma 1×1 branca é bindada.
@group(2) @binding(0) var albedo_tex: texture_2d<f32>;
@group(2) @binding(1) var albedo_samp: sampler;

// vertex do SHADOW PASS: projeta pela luz (só posição).
@vertex
fn shadow_vs(
  @location(0) position: vec3<f32>,
  @location(2) m0: vec4<f32>,
  @location(3) m1: vec4<f32>,
  @location(4) m2: vec4<f32>,
  @location(5) m3: vec4<f32>,
) -> @builtin(position) vec4<f32> {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  return cam.light_vp * (model * vec4<f32>(position, 1.0));
}

// fator de sombra (1 = iluminado, 0 = na sombra) via PCF 3×3.
fn shadow_factor(world: vec3<f32>) -> f32 {
  let lc = cam.light_vp * vec4<f32>(world, 1.0);
  let proj = lc.xyz / lc.w;
  let uv = proj.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || proj.z > 1.0) { return 1.0; }
  let d = proj.z - 0.0015;   // bias contra acne
  let texel = 1.0 / 2048.0;
  var sum = 0.0;
  for (var oy = -1; oy <= 1; oy = oy + 1) {
    for (var ox = -1; ox <= 1; ox = ox + 1) {
      let o = vec2<f32>(f32(ox), f32(oy)) * texel;
      sum = sum + textureSampleCompare(shadow_tex, shadow_samp, uv + o, d);
    }
  }
  return sum / 9.0;
}

// ── SKYBOX: triângulo fullscreen; gradiente + estrelas por DIREÇÃO de mundo
//    (giram junto com a câmera). Depth write off (fica no fundo). ──────────────
struct SkyOut { @builtin(position) clip: vec4<f32>, @location(0) ndc: vec2<f32> };
@vertex
fn sky_vs(@builtin(vertex_index) vi: u32) -> SkyOut {
  var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  var o: SkyOut;
  o.clip = vec4<f32>(p[vi], 1.0, 1.0);
  o.ndc = p[vi];
  return o;
}
fn hash13(p3: vec3<f32>) -> f32 {
  var q = fract(p3 * 0.1031);
  q = q + dot(q, q.yzx + 33.33);
  return fract((q.x + q.y) * q.z);
}
@fragment
fn sky_fs(i: SkyOut) -> @location(0) vec4<f32> {
  let ray = normalize(cam.cam_fwd.xyz
    + cam.cam_right.xyz * (i.ndc.x * cam.cam_right.w)
    + cam.cam_up.xyz * (i.ndc.y * cam.cam_up.w));
  let t = clamp(ray.y * 0.5 + 0.5, 0.0, 1.0);
  var col = mix(vec3<f32>(0.02, 0.02, 0.035), vec3<f32>(0.01, 0.015, 0.05), t);
  // estrelas: hash da direção quantizada (esparsas e brilhantes)
  let h = hash13(floor(ray * 260.0));
  if (h > 0.9915) { let s = (h - 0.9915) * 110.0; col = col + vec3<f32>(s, s, s); }
  return vec4<f32>(col, 1.0);
}

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) color: vec4<f32>,
  @location(2) world: vec3<f32>,
  @location(3) emissive: f32,
  @location(4) tex: f32,
  @location(5) uv: vec2<f32>,
};

@vertex
fn vs(
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(8) uv: vec2<f32>,
  @location(2) m0: vec4<f32>,
  @location(3) m1: vec4<f32>,
  @location(4) m2: vec4<f32>,
  @location(5) m3: vec4<f32>,
  @location(6) color: vec4<f32>,
  @location(7) iparams: vec4<f32>,
) -> VOut {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  let world = model * vec4<f32>(position, 1.0);
  var o: VOut;
  o.clip = cam.view_proj * world;
  o.normal = normalize((model * vec4<f32>(normal, 0.0)).xyz);
  o.color = color;
  o.world = world.xyz;
  o.emissive = iparams.x;
  o.tex = iparams.y;
  o.uv = uv;
  return o;
}

// ÁGUA INSTANCIADA: instância = UM vec4 direto do storage buffer da física
// (xyz = centro, w = densidade ASSINADA — w<0 significa "cercada nos 8
// octantes", invisível de qualquer ângulo). O culling de casca roda AQUI:
// partícula cercada colapsa em ponto (escala 0) e o rasterizador a descarta
// sem gerar um fragmento. 1 draw call, zero readback, zero FFI por partícula.
@vertex
fn vs_water(
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(8) uv: vec2<f32>,
  @location(2) ipos: vec4<f32>,
) -> VOut {
  let s = select(cam.water.x, 0.0, ipos.w < 0.0);
  let world = ipos.xyz + position * s;
  var o: VOut;
  o.clip = cam.view_proj * vec4<f32>(world, 1.0);
  o.normal = normal;                       // escala uniforme: normal intacta
  // MESMA fórmula de cor do desenho por partícula antigo (r/b fixos, só o
  // verde clareia de leve com a altura) — o gradiente forte de antes fazia o
  // topo parecer OUTRO líquido.
  let shade = clamp(ipos.y * 0.026, 0.0, 0.16);
  o.color = vec4<f32>(0.22, 0.494 + shade, 0.894, 1.0);
  o.world = world;
  o.emissive = 0.0;
  o.tex = 0.0;
  o.uv = uv;
  return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
  var albedo = i.color.rgb;
  // UV-CORRETO: amostra a textura de albedo pela UV per-vértice (interpolada) —
  // mapeamento do modelo (OBJ vt / UVs geradas dos primitivos). Amostrada SEMPRE
  // (control flow uniforme p/ as derivadas do sampler); só APLICADA se tex real.
  let texcol = textureSample(albedo_tex, albedo_samp, i.uv).rgb;
  // i.tex: 0=nenhuma, 1=xadrez procedural, >=2 = textura real (imagem).
  if (i.tex > 1.5) {
    albedo = albedo * texcol;
  } else if (i.tex > 0.5) {
    let s = 1.0;
    let c = floor(i.world.x * s) + floor(i.world.z * s) + floor(i.world.y * s);
    let chk = fract(c * 0.5) * 2.0;        // 0 ou 1
    albedo = albedo * mix(0.5, 1.0, chk);
  }
  // emissivo (ex.: o Sol) — cor cheia, sem sombreamento
  if (i.emissive > 0.5) { return vec4<f32>(albedo, i.color.a); }
  let n = normalize(i.normal);
  // LUZ PONTUAL: cam.light.xyz = POSICAO da luz (ex.: o Sol)
  let l = normalize(cam.light.xyz - i.world);
  let nd = max(dot(n, l), 0.0);
  let sh = shadow_factor(i.world);          // 1 = iluminado, 0 = na sombra
  let lit = cam.light.w + (1.0 - cam.light.w) * nd * sh;
  let vdir = normalize(cam.cam_pos.xyz - i.world);
  let h = normalize(l + vdir);
  let spec = pow(max(dot(n, h), 0.0), 32.0) * 0.3 * sh;
  let rgb = albedo * lit + vec3<f32>(spec, spec, spec);
  return vec4<f32>(rgb, i.color.a);
}
"#;
