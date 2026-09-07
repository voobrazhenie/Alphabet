// Post pass — a port of <script id="fsPost"> in neurons.html.
//
// No FXAA: a plain read. When the texture is larger than the output the bilinear
// sampler resolves it, which for an exact 2x is a box filter.

struct Post {
  outRes: vec2f,   // size being drawn
  texel: vec2f,    // 1 / size of the texture being read
  fxaa: f32,
  pad0: f32,
  pad1: f32,
  pad2: f32,
};

@group(0) @binding(0) var<uniform> P: Post;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

const LUMA = vec3f(0.299, 0.587, 0.114);

@vertex
fn vsMain(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
  let x = select(-1.0, 3.0, vi == 1u);
  let y = select(-1.0, 3.0, vi == 2u);
  return vec4f(x, y, 0.0, 1.0);
}

@fragment
fn fsMain(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  // The scene pass wrote this texture with the same top-down convention the
  // sampler reads it in, so there is no flip here.
  let uv = pos.xy / P.outRes;
  if (P.fxaa < 0.5) {
    return textureSample(tex, samp, uv);
  }
  let inv = P.texel;
  let mC = textureSample(tex, samp, uv).rgb;
  let nw = textureSample(tex, samp, uv + vec2f(-1.0, -1.0) * inv).rgb;
  let ne = textureSample(tex, samp, uv + vec2f( 1.0, -1.0) * inv).rgb;
  let sw = textureSample(tex, samp, uv + vec2f(-1.0,  1.0) * inv).rgb;
  let se = textureSample(tex, samp, uv + vec2f( 1.0,  1.0) * inv).rgb;
  let lM  = dot(mC, LUMA);
  let lNW = dot(nw, LUMA); let lNE = dot(ne, LUMA);
  let lSW = dot(sw, LUMA); let lSE = dot(se, LUMA);
  let lMin = min(lM, min(min(lNW, lNE), min(lSW, lSE)));
  let lMax = max(lM, max(max(lNW, lNE), max(lSW, lSE)));

  var dir = vec2f(-((lNW + lNE) - (lSW + lSE)), ((lNW + lSW) - (lNE + lSE)));
  let reduce = max((lNW + lNE + lSW + lSE) * 0.03125, 0.0078125);
  let rcp = 1.0 / (min(abs(dir.x), abs(dir.y)) + reduce);
  dir = clamp(dir * rcp, vec2f(-8.0), vec2f(8.0)) * inv;

  let a = 0.5 * (textureSample(tex, samp, uv + dir * (1.0/3.0 - 0.5)).rgb +
                 textureSample(tex, samp, uv + dir * (2.0/3.0 - 0.5)).rgb);
  let b = a * 0.5 + 0.25 * (textureSample(tex, samp, uv - dir * 0.5).rgb +
                            textureSample(tex, samp, uv + dir * 0.5).rgb);
  let lB = dot(b, LUMA);
  let outc = select(b, a, lB < lMin || lB > lMax);
  return vec4f(outc, 1.0);
}
