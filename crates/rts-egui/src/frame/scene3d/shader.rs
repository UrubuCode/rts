/// WGSL: vertex = pos+normal (slot 0) × instância model(4×vec4)+color (slot 1).
/// Uniform (group 0): `Cam` (um slot de 256 bytes por vista, offset dinâmico)
/// + `Env` (luzes, céu, neblina — um por frame).
pub(in crate::frame::scene3d) const SHADER: &str = r#"
struct Cam {
  view_proj: mat4x4<f32>,
  light: vec4<f32>,      // luz LEGADA (setLight): xyz = posição, w = ambiente
  cam_pos: vec4<f32>,
  cam_right: vec4<f32>,  // w = tanH
  cam_up: vec4<f32>,     // w = tanV
  cam_fwd: vec4<f32>,
  light_vp: mat4x4<f32>, // view·proj da luz (shadow map)
  water: vec4<f32>,      // x = escala da partícula de água
  view_bg: vec4<f32>,    // rgb = cor do fundo; w = 1 → fundo chapado nesta vista
  proj: vec4<f32>,       // x = 1 ortográfica; y = meia altura; z = meia largura
};
struct LuzGpu { a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, d: vec4<f32> };
struct Env {
  info: vec4<f32>,        // x = nº de luzes, y = índice da luz com sombra (-1), z = modo do ambiente, w = intensidade
  amb: vec4<f32>,         // rgb = cor do ambiente
  sky0: vec4<f32>,        // x = modo do céu, y = exposição, z = estrelas, w = tamanho do sol (rad)
  sky_top: vec4<f32>,
  sky_horizon: vec4<f32>,
  sky_ground: vec4<f32>,
  sun_dir: vec4<f32>,     // xyz = direção em que a luz do sol viaja; w = 1 se há panorama
  fog: vec4<f32>,         // rgb = cor, w = densidade
  lights: array<LuzGpu, 8>,
};
@group(0) @binding(0) var<uniform> cam: Cam;
@group(0) @binding(1) var<uniform> env: Env;
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
fn estrelas(ray: vec3<f32>) -> f32 {
  let h = hash13(floor(ray * 260.0));
  return select(0.0, (h - 0.9915) * 110.0, h > 0.9915);
}
fn equiret(ray: vec3<f32>) -> vec2<f32> {
  return vec2<f32>(atan2(ray.x, ray.z) * 0.15915494 + 0.5, acos(clamp(ray.y, -1.0, 1.0)) * 0.31830989);
}
fn cor_do_ceu(ray: vec3<f32>, pano: vec3<f32>) -> vec3<f32> {
  let modo = env.sky0.x;
  if (modo < 0.5) {
    // 0: o céu estrelado de antes, idêntico (sem exposição)
    let t = clamp(ray.y * 0.5 + 0.5, 0.0, 1.0);
    return mix(vec3<f32>(0.02, 0.02, 0.035), vec3<f32>(0.01, 0.015, 0.05), t) + vec3<f32>(estrelas(ray));
  }
  var col = env.sky_top.rgb;                     // 2: cor
  if (modo < 1.5) {                              // 1: procedural com disco do sol
    let acima = mix(env.sky_horizon.rgb, env.sky_top.rgb, sqrt(clamp(ray.y, 0.0, 1.0)));
    let abaixo = mix(env.sky_horizon.rgb, env.sky_ground.rgb, sqrt(clamp(-ray.y, 0.0, 1.0)));
    col = select(abaixo, acima, ray.y >= 0.0);
    let tam = max(env.sky0.w, 0.001);
    let disco = smoothstep(cos(tam * 1.6), cos(tam), dot(ray, -normalize(env.sun_dir.xyz)));
    col = col + vec3<f32>(1.0, 0.95, 0.85) * (disco * 3.0);
  } else if (modo > 2.5) {                       // 3: panorama (sem textura: a cor do topo)
    col = select(env.sky_top.rgb, pano, env.sun_dir.w > 0.5);
  }
  return (col + vec3<f32>(estrelas(ray) * env.sky0.z)) * env.sky0.y;
}
@fragment
fn sky_fs(i: SkyOut) -> @location(0) vec4<f32> {
  var ray = normalize(cam.cam_fwd.xyz
    + cam.cam_right.xyz * (i.ndc.x * cam.cam_right.w)
    + cam.cam_up.xyz * (i.ndc.y * cam.cam_up.w));
  if (cam.proj.x > 0.5) { ray = normalize(cam.cam_fwd.xyz); }   // ortográfica: um raio só
  // amostra SEMPRE (fluxo uniforme); só vale no modo panorama
  let pano = textureSample(albedo_tex, albedo_samp, equiret(ray)).rgb;
  if (cam.view_bg.w > 0.5) { return vec4<f32>(cam.view_bg.rgb, 1.0); }
  return vec4<f32>(cor_do_ceu(ray, pano), 1.0);
}

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) color: vec4<f32>,
  @location(2) world: vec3<f32>,
  @location(3) emissive: f32,
  @location(4) tex: f32,
  @location(5) uv: vec2<f32>,
  // > 0: UV em coordenada de MUNDO (repetições por unidade), projetada pelo
  // eixo dominante da normal — textura repete numa caixa grande em vez de esticar.
  @location(6) tile: f32,
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
  o.tile = iparams.z;
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
  o.tile = 0.0;
  return o;
}

// Espelham `lights.rs` (attenuation, spot_factor, fog_factor): os testes de lá
// fixam os números destas contas.
fn atenuacao(d: f32, alcance: f32) -> f32 {
  if (alcance <= 0.0) { return 0.0; }
  let r = d / alcance;
  let x = clamp(1.0 - r * r, 0.0, 1.0);
  return x * x;
}
fn cone(c: f32, c_in: f32, c_out: f32) -> f32 {
  if (c_in - c_out <= 0.0001) { return select(0.0, 1.0, c >= c_out); }
  let t = clamp((c - c_out) / (c_in - c_out), 0.0, 1.0);
  return t * t * (3.0 - 2.0 * t);
}
fn ambiente(n: vec3<f32>) -> vec3<f32> {
  let modo = env.info.z;
  if (modo < 0.5) { return vec3<f32>(cam.light.w); }            // escalar do setLight
  if (modo < 1.5) { return env.amb.rgb * env.info.w; }          // cor
  return mix(env.sky_ground.rgb, env.sky_top.rgb, n.y * 0.5 + 0.5) * env.info.w;   // céu
}
fn neblina(rgb: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
  if (env.fog.w <= 0.0) { return rgb; }
  return mix(env.fog.rgb, rgb, exp(-env.fog.w * length(cam.cam_pos.xyz - world)));
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
  var albedo = i.color.rgb;
  // UV-CORRETO: amostra a textura de albedo pela UV per-vértice (interpolada) —
  // mapeamento do modelo (OBJ vt / UVs geradas dos primitivos). Amostrada SEMPRE
  // (control flow uniforme p/ as derivadas do sampler); só APLICADA se tex real.
  // UV em mundo (tile > 0): o eixo dominante da normal escolhe o plano. Só
  // valores mudam aqui; a amostra continua em fluxo uniforme.
  let an = abs(i.normal);
  let face_y = an.y >= an.x && an.y >= an.z;
  let face_x = !face_y && an.x >= an.z;
  let uv_mundo = select(select(i.world.xy, i.world.zy, face_x), i.world.xz, face_y) * i.tile;
  let uv = select(i.uv, uv_mundo, i.tile > 0.0);
  let texcol = textureSample(albedo_tex, albedo_samp, uv).rgb;
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
  let sh = shadow_factor(i.world);          // uma amostra, em fluxo uniforme
  let vdir = normalize(cam.cam_pos.xyz - i.world);
  var rgb: vec3<f32>;
  let nluzes = min(u32(env.info.x), 8u);
  if (nluzes == 0u) {
    // LEGADO (setLight): exatamente o shading de antes
    let l = normalize(cam.light.xyz - i.world);
    let nd = max(dot(n, l), 0.0);
    let lit = cam.light.w + (1.0 - cam.light.w) * nd * sh;
    let h = normalize(l + vdir);
    let spec = pow(max(dot(n, h), 0.0), 32.0) * 0.3 * sh;
    rgb = albedo * lit + vec3<f32>(spec, spec, spec);
  } else {
    var dif = vec3<f32>(0.0, 0.0, 0.0);
    var spec = 0.0;
    let idx_sombra = i32(env.info.y);
    for (var k = 0u; k < nluzes; k = k + 1u) {
      let luz = env.lights[k];
      var l = -normalize(luz.b.xyz);            // direcional: contra a direção da luz
      var att = 1.0;
      if (luz.a.w > 0.5) {                      // pontual ou spot
        let para = luz.a.xyz - i.world;
        let dist = length(para);
        l = para / max(dist, 0.0001);
        att = atenuacao(dist, luz.b.w);
        if (luz.a.w > 1.5) { att = att * cone(dot(-l, normalize(luz.b.xyz)), luz.d.x, luz.d.y); }
      }
      let s = select(1.0, sh, i32(k) == idx_sombra);
      let nd = max(dot(n, l), 0.0);
      dif = dif + luz.c.rgb * (luz.c.w * nd * att * s);
      let h = normalize(l + vdir);
      spec = spec + pow(max(dot(n, h), 0.0), 32.0) * 0.3 * s * att * luz.c.w;
    }
    rgb = albedo * (ambiente(n) + dif) + vec3<f32>(spec, spec, spec);
  }
  return vec4<f32>(neblina(rgb, i.world), i.color.a);
}
"#;
