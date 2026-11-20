// Layer compositing, blend modes and stroke commits. Every texture here is the canvas's size, so
// pixels are read with textureLoad at the fragment's own position: no filtering, no coordinates.
// Colours are premultiplied; blend modes work on straight colour (separable and non-separable
// modes as usually defined for compositing).

struct LayerUniform {
    opacity: f32,
    // 0: no stroke on this layer, 1: the stroke paints, 2: it erases, 3: it paints alpha-locked,
    // 4: it takes the layer's place where it reaches
    stroke_mode: u32,
    stroke_opacity: f32,
    blend: u32,
    // how the layer is clipped: CLIP_NONE, CLIP_BY_BASE, CLIP_ATOP or CLIP_BASE
    clipped: u32,
    // the stroke's texturized grain: -1 for none, else a layer of `grains`
    grain: i32,
    grain_scale: f32,
    grain_depth: f32,
    // premultiplied colour for fs_solid
    solid: vec4<f32>,
    // the stroke's wet edges (0..1, negative as a preview draws them) and burnt edges (0..1), its
    // alpha threshold (negative for none), and its grain's brightness and contrast (-1..1)
    wet: f32,
    burnt: f32,
    threshold: f32,
    grain_brightness: f32,
    grain_contrast: f32,
    // 1: the layer shows through its mask (mask_tex); 1: the stroke being drawn is on the mask
    masked: f32,
    stroke_on_mask: f32,
    // how the stroke blends into the layer: a blend mode's number (0: normal) in the low byte,
    // with STROKE_LUMINANCE for luminance blending
    stroke_blend: u32,
    // an onion skin's colour (rgb), screened over the layer, and how much (a)
    tint: vec4<f32>,
    // the stroke's texturized grain turned, radians, and how it and the burnt edges combine with
    // the stroke (blend modes' numbers)
    grain_rotation: f32,
    grain_blend: u32,
    burnt_blend: u32,
    // 1: the grain read unfiltered (sharp)
    grain_sharp: f32,
    // a dual brush's second stroke (second_tex): how its coverage combines with the first's (a
    // blend mode's number + 1; 0: none), and where it is cut first (negative: not cut)
    dual: u32,
    dual_threshold: f32,
    // 1: the clip base shows through its own mask (clip_mask_tex), which clipping follows
    clip_masked: f32,
    // pixels a side of the image the grain is read from
    grain_side: f32,
    // how many of this canvas's pixels a pixel of the canvas its strokes are given in takes, and
    // where this one's top left is in those (x: the scale, yz: the point): grain fixed to the
    // canvas is read where it is on that canvas
    scaled: vec4<f32>,
    // the paper: how strongly its hollows take the stroke's paint away (0: the stroke takes no
    // paper), how many canvas pixels a pixel of its tile covers, and how deep it shows in the
    // background (0: a flat colour)
    paper: vec4<f32>,
    // non-destructive keyframe transform (inverse 2D affine map from canvas pixel to layer pixel):
    // xform0 = (a, b, c, d), xform1 = (e, f, active: 1.0 when transformed, 0.0 for identity)
    xform0: vec4<f32>,
    xform1: vec4<f32>,
    // v2.0 non-destructive live filters & track layer effects (64 bytes -> total 256 bytes = SLOT):
    // fx0 = (hue_rad, saturation, brightness, blur_radius_px)
    // fx1 = (sharpen, noise, outline_width_px, shadow_blur_px)
    // fx2 = (shadow_offset_x, shadow_offset_y, glow_radius_px, glow_intensity_signed)
    // fx3 = (outline_rgba8, shadow_rgba8, glow_rgba8, overlay_rgba8)
    fx0: vec4<f32>,
    fx1: vec4<f32>,
    fx2: vec4<f32>,
    fx3: vec4<u32>,
}

// in stroke_blend: the stroke mixed in by its coverage in squared colour (nearer to light)
const STROKE_LUMINANCE: u32 = 256u;

// in clipped: not clipped; multiplied by the alpha of the base it is clipped to (clip_tex), drawn
// without it; blended onto its base (the backdrop) in their clipping group; the group's base,
// written as it is (fs_blend, nothing read below)
const CLIP_NONE: u32 = 0u;
const CLIP_BY_BASE: u32 = 1u;
const CLIP_ATOP: u32 = 2u;
const CLIP_BASE: u32 = 3u;

@group(0) @binding(0) var layer_tex: texture_2d<f32>;
@group(0) @binding(1) var stroke_tex: texture_2d<f32>;
@group(0) @binding(2) var clip_tex: texture_2d<f32>;
@group(0) @binding(3) var backdrop_tex: texture_2d<f32>;
@group(0) @binding(4) var grains: texture_2d_array<f32>;
@group(0) @binding(5) var grain_smp: sampler;
@group(0) @binding(6) var mask_tex: texture_2d<f32>;
@group(0) @binding(7) var second_tex: texture_2d<f32>;
@group(0) @binding(8) var clip_mask_tex: texture_2d<f32>;
@group(0) @binding(9) var paper_tex: texture_2d<f32>;
@group(0) @binding(10) var layer_smp: sampler;
@group(1) @binding(0) var<uniform> lu: LayerUniform;

// one triangle covering the target
@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

// the stroke's coverage at p moved by o, held at the edges
fn stroke_alpha(p: vec2<i32>, o: vec2<i32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(stroke_tex));
    return textureLoad(stroke_tex, clamp(p + o, vec2<i32>(0), dims - 1), 0).a;
}

// a grain's value made lighter or darker and of more or less contrast
fn grain_look(g: f32) -> f32 {
    return clamp((g - 0.5) * (1.0 + lu.grain_contrast) + 0.5 + lu.grain_brightness, 0.0, 1.0);
}

// the stroke at a pixel, and how much its burnt edges show there
struct StrokePixel {
    s: vec4<f32>,
    burn: f32,
}

// the stroke at a pixel: its build-up, its edges, the paper grain fixed to the canvas, its
// threshold and its opacity. When `use_bilinear` is true (Camera/Perform transform active),
// samples `stroke_tex` and `second_tex` with hardware bilinear filtering at subpixel `sf`
// to match `load_layer_at_f` cleanly with zero jaggedness or commit jump.
fn stroke_pixel_ex(p: vec2<i32>, sf: vec2<f32>, use_bilinear: bool) -> StrokePixel {
    var s = textureLoad(stroke_tex, p, 0);
    if (use_bilinear) {
        let sdims = vec2<f32>(textureDimensions(stroke_tex));
        s = textureSampleLevel(stroke_tex, layer_smp, sf / sdims, 0.0);
    }
    // what the stamps laid, before the edges, grain and second change it
    let laid = s.a;
    if (lu.wet < 0.0 && s.a > 0.0) {
        // a preview: how much of an edge the pixel is (its coverage above its neighbours' a few
        // pixels away), the edges a little stronger and the inside as it is (as measured: at
        // full, a wide brush's rim 4% stronger, its middle and a thin line unchanged)
        let d = 3;
        let around = 0.25 * (stroke_alpha(p, vec2<i32>(d, 0)) + stroke_alpha(p, vec2<i32>(-d, 0)) + stroke_alpha(p, vec2<i32>(0, d)) + stroke_alpha(p, vec2<i32>(0, -d)));
        let edge = clamp((s.a - around) * 2.0, 0.0, 1.0);
        let wet_a = min(s.a * (1.0 - lu.wet * 0.1 * edge), 1.0);
        s = s * (wet_a / s.a);
    } else if (lu.wet > 0.0 && s.a > 0.0) {
        // on the canvas the paint pools (see pooled_over)
        s = s * mix(1.0, pooled_over(s.a), lu.wet);
    }
    let built = s.a;
    let sample_at = select(vec2<f32>(p) + 0.5, sf, use_bilinear);
    if (lu.grain >= 0) {
        let at = sample_at / lu.scaled.x + lu.scaled.yz;
        let gc = cos(lu.grain_rotation);
        let gs = sin(lu.grain_rotation);
        let uv = vec2<f32>(at.x * gc - at.y * gs, at.x * gs + at.y * gc) / lu.grain_scale;
        let sharp = lu.grain_sharp > 0.5;
        // a brush file's own image has the fine detail itself; ours only stands in for it
        let own = lu.grain_side > 256.0;
        let level = max(log2(lu.grain_side / lu.grain_scale), 0.0);
        var raw = textureSampleLevel(grains, grain_smp, uv, lu.grain, select(level, 0.0, sharp && !own)).r;
        if (sharp) {
            if (own) {
                // unfiltered: the texel under it at its level
                let l = i32(round(level));
                raw = textureLoad(grains, vec2<i32>(fract(uv) * vec2<f32>(textureDimensions(grains, l))), lu.grain, l).r;
            } else {
                raw = sharp_grain(raw, uv);
            }
        }
        let g = grain_look(raw);
        // the coverage and the grain combined by the grain's blend mode, as deep as it says, and
        // only where the stroke is
        let a = grain_combine(s.a, g, lu.grain_depth, lu.grain_blend);
        s = s * (a / max(s.a, 1e-6));
    }
    // the paper, fixed to the canvas, whatever the brush: its hollows take a share of the paint
    if (lu.paper.x > 0.0) {
        let at = sample_at / lu.scaled.x + lu.scaled.yz;
        s = s * (1.0 - lu.paper.x * (1.0 - paper_height(at)));
    }
    // a dual brush's second: its coverage (cut at its threshold) combined with the first's by
    // the dual mode, or by Normal showing where the first doesn't
    if (lu.dual != 0u) {
        var t = textureLoad(second_tex, p, 0);
        if (use_bilinear) {
            let ddims = vec2<f32>(textureDimensions(second_tex));
            t = textureSampleLevel(second_tex, layer_smp, sf / ddims, 0.0);
        }
        var b = t.a;
        if (lu.dual_threshold >= 0.0) {
            b = select(0.0, 1.0, b > lu.dual_threshold);
        }
        let mode = dual_mode(lu.dual);
        let a = dual_rise(dual_combine(s.a, b, mode), built, mode);
        // the first's colour; by Normal turned toward the second's (drawn in the stroke's colour)
        // as far as the second shows where the first laid little, so where the first fades out
        // the two meet smoothly, not at a seam of whatever colour the first's faintest paint
        // holds; the second's wherever the first left nothing
        let toward = select(select(0.0, clamp((1.0 - laid) * b, 0.0, 1.0), mode == 0u), 1.0, s.a <= 0.0);
        let first_colour = s.rgb / max(s.a, 1e-6);
        let second_colour = t.rgb / max(t.a, 1e-6);
        var colour = mix(first_colour, second_colour, toward);
        if ((lu.dual & DUAL_CLASSIC) != 0u && s.a > 0.0) {
            // the classic combine: the first's colour whole and the second's share added up
            colour = (first_colour + second_colour * toward) / (1.0 + toward);
        }
        s = vec4<f32>(colour * a, a);
    }
    // burnt edges show where the stroke is thin: where its paper grain or its second took paint
    // away, or without either wherever it is less than full
    var burn = 0.0;
    if (lu.burnt > 0.0 && built > 0.0) {
        burn = lu.burnt * select(1.0 - s.a, abs(built - s.a), lu.grain >= 0 || lu.dual != 0u);
    }
    if (lu.threshold >= 0.0 && s.a > 0.0) {
        let on = select(0.0, 1.0, s.a > lu.threshold);
        s = s * (on / s.a);
    }
    return StrokePixel(s * lu.stroke_opacity, burn);
}

fn stroke_pixel(p: vec2<i32>) -> StrokePixel {
    return stroke_pixel_ex(p, vec2<f32>(p) + 0.5, false);
}

fn stroke_at(p: vec2<i32>) -> vec4<f32> {
    return stroke_pixel(p).s;
}

// burnt edges: the stroke's result `r` blended once more with the paint that was under it, by
// `w` (nothing shows where the layer was empty)
fn burnt(under: vec4<f32>, r: vec4<f32>, w: f32) -> vec4<f32> {
    if (w <= 0.0) {
        return r;
    }
    let b = blend(under, r, lu.burnt_blend);
    return mix(r, vec4<f32>(unpremultiply(b) * r.a, r.a), clamp(w, 0.0, 1.0));
}

// a layer with the stroke in progress on it applied
fn with_stroke_ex(c0: vec4<f32>, p: vec2<i32>, sf: vec2<f32>, use_bilinear: bool) -> vec4<f32> {
    var c = c0;
    if (lu.stroke_mode == 4u) {
        // where it reaches its alpha is marked by 1, so that clear can be laid down as well
        let t = textureLoad(stroke_tex, p, 0);
        return select(c0, vec4<f32>(t.rgb, t.a - 1.0), t.a >= 0.5);
    }
    if (lu.stroke_mode == 5u) {
        // the same in a stroke of 8 bits a channel: its alpha a half more, halved
        let t = textureLoad(stroke_tex, p, 0);
        return select(c0, vec4<f32>(t.rgb, 2.0 * t.a - 1.0), t.a >= 0.25);
    }
    if (lu.stroke_mode != 0u) {
        let px = stroke_pixel_ex(p, sf, use_bilinear);
        let s = px.s;
        // where the stroke left nothing (no paint, no burnt edge) the layer is as it was
        if (s.a <= 0.0 && px.burn <= 0.0) {
            return c0;
        }
        if (lu.stroke_mode == 2u) {
            c = c * (1.0 - s.a);
        } else {
            // painted (1), or recoloured keeping the layer's alpha (3): the blend written once, as
            // Direct3D 12's compiler takes a long while over each copy of its modes
            let painted = burnt(c0, stroke_into(c, s), px.burn);
            c = select(painted, painted * (c.a / max(painted.a, 1e-6)), lu.stroke_mode == 3u);
        }
    }
    return c;
}

fn with_stroke(c0: vec4<f32>, p: vec2<i32>) -> vec4<f32> {
    return with_stroke_ex(c0, p, vec2<f32>(p) + 0.5, false);
}

fn load_layer_at_f(tex: texture_2d<f32>, pf: vec2<f32>) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(tex));
    var pt = pf + 0.5;
    if (lu.xform1.z > 0.5) {
        pt = vec2<f32>(
            lu.xform0.x * pt.x + lu.xform0.z * pt.y + lu.xform1.x,
            lu.xform0.y * pt.x + lu.xform0.w * pt.y + lu.xform1.y,
        );
    }
    if (pt.x < 0.0 || pt.y < 0.0 || pt.x > dims.x || pt.y > dims.y) {
        return vec4<f32>(0.0);
    }
    return textureSampleLevel(tex, layer_smp, pt / dims, 0.0);
}

fn load_layer_at(tex: texture_2d<f32>, p: vec2<i32>) -> vec4<f32> {
    if (lu.xform1.z <= 0.5) {
        return textureLoad(tex, p, 0);
    }
    return load_layer_at_f(tex, vec2<f32>(p));
}

fn over_premul(top: vec4<f32>, bottom: vec4<f32>) -> vec4<f32> {
    return top + bottom * (1.0 - top.a);
}

fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.bg, k.wz), vec4<f32>(c.gb, k.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    let e = 1e-6;
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + k.xyz) * 6.0 - k.www);
    return c.z * mix(k.xxx, clamp(p - k.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}

fn sample_layer_filtered(tex: texture_2d<f32>, p: vec2<i32>) -> vec4<f32> {
    let blur = lu.fx0.w;
    let sharp = lu.fx1.x;
    if (blur > 0.05) {
        // Hardware-bilinear 9-tap circular Gaussian approximation (9 TMU ops vs 68 textureLoads)
        let pf = vec2<f32>(p);
        let r1 = blur * 0.55;
        let d2 = blur * 0.70710678;
        var acc = load_layer_at_f(tex, pf) * 0.24;
        acc += (load_layer_at_f(tex, pf + vec2<f32>(r1, 0.0))
            + load_layer_at_f(tex, pf + vec2<f32>(-r1, 0.0))
            + load_layer_at_f(tex, pf + vec2<f32>(0.0, r1))
            + load_layer_at_f(tex, pf + vec2<f32>(0.0, -r1))) * 0.11;
        acc += (load_layer_at_f(tex, pf + vec2<f32>(d2, d2))
            + load_layer_at_f(tex, pf + vec2<f32>(-d2, d2))
            + load_layer_at_f(tex, pf + vec2<f32>(d2, -d2))
            + load_layer_at_f(tex, pf + vec2<f32>(-d2, -d2))) * 0.08;
        return acc;
    }
    var c = load_layer_at(tex, p);
    if (sharp > 0.01) {
        let pf = vec2<f32>(p);
        let avg = 0.25 * (
            load_layer_at_f(tex, pf + vec2<f32>(1.0, 0.0))
            + load_layer_at_f(tex, pf + vec2<f32>(-1.0, 0.0))
            + load_layer_at_f(tex, pf + vec2<f32>(0.0, 1.0))
            + load_layer_at_f(tex, pf + vec2<f32>(0.0, -1.0))
        );
        let boosted = clamp(c + (c - avg) * (sharp * 2.0), vec4<f32>(0.0), vec4<f32>(1.0));
        c = vec4<f32>(boosted.rgb, c.a);
    }
    return c;
}

fn apply_live_color_filters(c0: vec4<f32>, p: vec2<i32>) -> vec4<f32> {
    var c = c0;
    if (c.a <= 1e-5) {
        return c;
    }
    let has_hsb = abs(lu.fx0.x) > 0.001 || abs(lu.fx0.y - 1.0) > 0.001 || abs(lu.fx0.z) > 0.001;
    let noise_amt = lu.fx1.y;
    let overlay = unpack4x8unorm(lu.fx3.w);
    if (!has_hsb && noise_amt <= 0.001 && overlay.a <= 0.001) {
        return c;
    }
    var rgb = c.rgb / max(c.a, 1e-6);
    if (has_hsb) {
        var hsv = rgb_to_hsv(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
        hsv.x = fract(hsv.x + lu.fx0.x / 6.2831853);
        hsv.y = clamp(hsv.y * lu.fx0.y, 0.0, 1.0);
        hsv.z = clamp(hsv.z + lu.fx0.z, 0.0, 1.0);
        rgb = hsv_to_rgb(hsv);
    }
    if (noise_amt > 0.001) {
        let h = fract(sin(dot(vec2<f32>(p), vec2<f32>(12.9898, 78.233))) * 43758.5453) * 2.0 - 1.0;
        rgb = clamp(rgb + vec3<f32>(h * noise_amt * 0.35), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if (overlay.a > 0.001) {
        rgb = mix(rgb, overlay.rgb, overlay.a);
    }
    return vec4<f32>(rgb * c.a, c.a);
}

fn apply_track_effects(tex: texture_2d<f32>, base_c: vec4<f32>, p: vec2<i32>) -> vec4<f32> {
    // Fast exit when no spatial track effects (outline, shadow, glow) are enabled
    if (lu.fx3.x == 0u && lu.fx3.y == 0u && lu.fx3.z == 0u) {
        return base_c;
    }
    var out = base_c;
    let pf = vec2<f32>(p);

    // 1. Stroke / Outline around alpha silhouette (early-out inside opaque strokes)
    let ow = lu.fx1.z;
    if (ow > 0.05 && lu.fx3.x != 0u && out.a < 0.995) {
        let ocol = unpack4x8unorm(lu.fx3.x);
        if (ocol.a > 0.001) {
            let od = ow * 0.70710678;
            var max_a = out.a;
            max_a = max(max_a, load_layer_at_f(tex, pf + vec2<f32>(ow, 0.0)).a);
            max_a = max(max_a, load_layer_at_f(tex, pf + vec2<f32>(-ow, 0.0)).a);
            max_a = max(max_a, load_layer_at_f(tex, pf + vec2<f32>(0.0, ow)).a);
            max_a = max(max_a, load_layer_at_f(tex, pf + vec2<f32>(0.0, -ow)).a);
            max_a = max(max_a, load_layer_at_f(tex, pf + vec2<f32>(od, od)).a);
            max_a = max(max_a, load_layer_at_f(tex, pf + vec2<f32>(-od, od)).a);
            max_a = max(max_a, load_layer_at_f(tex, pf + vec2<f32>(od, -od)).a);
            max_a = max(max_a, load_layer_at_f(tex, pf + vec2<f32>(-od, -od)).a);
            let oa = clamp(max_a * ocol.a, 0.0, 1.0);
            let outline_px = vec4<f32>(ocol.rgb * oa, oa);
            out = over_premul(out, outline_px);
        }
    }

    // 2. Outer / Inner Glow (8 hardware taps + opaque/transparent early-outs)
    let gr = lu.fx2.z;
    let gi = lu.fx2.w;
    if (gr > 0.05 && abs(gi) > 0.001 && lu.fx3.z != 0u) {
        let gcol = unpack4x8unorm(lu.fx3.z);
        let need_outer = gi > 0.0 && out.a < 0.995;
        let need_inner = gi < 0.0 && out.a > 0.005;
        if (gcol.a > 0.001 && (need_outer || need_inner)) {
            let gd = gr * 0.70710678;
            let avg_a = (
                load_layer_at_f(tex, pf + vec2<f32>(gr, 0.0)).a
                + load_layer_at_f(tex, pf + vec2<f32>(-gr, 0.0)).a
                + load_layer_at_f(tex, pf + vec2<f32>(0.0, gr)).a
                + load_layer_at_f(tex, pf + vec2<f32>(0.0, -gr)).a
                + load_layer_at_f(tex, pf + vec2<f32>(gd, gd)).a
                + load_layer_at_f(tex, pf + vec2<f32>(-gd, gd)).a
                + load_layer_at_f(tex, pf + vec2<f32>(gd, -gd)).a
                + load_layer_at_f(tex, pf + vec2<f32>(-gd, -gd)).a
            ) * 0.125;
            if (gi > 0.0) {
                let ga = clamp(avg_a * gi * gcol.a, 0.0, 1.0);
                let glow_px = vec4<f32>(gcol.rgb * ga, ga);
                out = over_premul(out, glow_px);
            } else {
                let edge = clamp((1.0 - avg_a) * (-gi) * gcol.a, 0.0, 1.0) * out.a;
                let rgb = mix(out.rgb / max(out.a, 1e-6), gcol.rgb, edge);
                out = vec4<f32>(rgb * out.a, out.a);
            }
        }
    }

    // 3. Drop Shadow (5 hardware taps + opaque early-out)
    if (lu.fx3.y != 0u && out.a < 0.995) {
        let scol = unpack4x8unorm(lu.fx3.y);
        if (scol.a > 0.001) {
            let sp = pf - lu.fx2.xy;
            let sb = lu.fx1.w;
            var sa = load_layer_at_f(tex, sp).a;
            if (sb > 0.05) {
                let sd = sb * 0.70710678;
                sa = (
                    sa * 2.0
                    + load_layer_at_f(tex, sp + vec2<f32>(sd, sd)).a
                    + load_layer_at_f(tex, sp + vec2<f32>(-sd, sd)).a
                    + load_layer_at_f(tex, sp + vec2<f32>(sd, -sd)).a
                    + load_layer_at_f(tex, sp + vec2<f32>(-sd, -sd)).a
                ) * (1.0 / 6.0);
            }
            let s_alpha = clamp(sa * scol.a, 0.0, 1.0);
            let shadow_px = vec4<f32>(scol.rgb * s_alpha, s_alpha);
            out = over_premul(out, shadow_px);
        }
    }

    return out;
}

// a layer as it composites: stroke, mask, clipping, live filters, track effects, opacity
fn layer_color(p: vec2<i32>) -> vec4<f32> {
    var c = sample_layer_filtered(layer_tex, p);
    var m = vec4<f32>(1.0);
    if (lu.masked > 0.5) {
        m = load_layer_at(mask_tex, p);
    }
    // the stroke goes into the layer, or into its mask: one place for it (its blend written once)
    let into_mask = lu.stroke_on_mask > 0.5;
    let transformed = lu.xform1.z > 0.5;
    var stroke_sf = vec2<f32>(p) + 0.5;
    var stroke_p = p;
    if (transformed) {
        let pt = vec2<f32>(p) + 0.5;
        stroke_sf = vec2<f32>(
            lu.xform0.x * pt.x + lu.xform0.z * pt.y + lu.xform1.x,
            lu.xform0.y * pt.x + lu.xform0.w * pt.y + lu.xform1.y,
        );
        stroke_p = vec2<i32>(floor(stroke_sf));
    }
    let dims = vec2<i32>(textureDimensions(layer_tex));
    let fdims = vec2<f32>(dims);
    var stroked = select(c, m, into_mask);
    let inside_id = stroke_p.x >= 0 && stroke_p.y >= 0 && stroke_p.x < dims.x && stroke_p.y < dims.y;
    let inside_xf = stroke_sf.x >= 0.0 && stroke_sf.y >= 0.0 && stroke_sf.x <= fdims.x && stroke_sf.y <= fdims.y;
    let inside_stroke = select(inside_id, inside_xf, transformed);
    if (inside_stroke) {
        let safe_p = clamp(stroke_p, vec2<i32>(0), dims - 1);
        stroked = with_stroke_ex(stroked, safe_p, stroke_sf, transformed);
    }
    if (into_mask) {
        m = stroked;
    } else {
        c = stroked;
    }
    if (lu.masked > 0.5) {
        // the mask shows the layer as much as it is light (and opaque)
        c = c * clamp(dot(m.rgb, vec3<f32>(0.299, 0.587, 0.114)), 0.0, 1.0);
    }
    c = apply_live_color_filters(c, p);
    c = apply_track_effects(layer_tex, c, p);
    if (lu.clipped == CLIP_BY_BASE) {
        var base = load_layer_at(clip_tex, p).a;
        if (lu.clip_masked > 0.5) {
            // the base as its mask shows it
            base = base * clamp(dot(load_layer_at(clip_mask_tex, p).rgb, vec3<f32>(0.299, 0.587, 0.114)), 0.0, 1.0);
        }
        c = c * base;
    }
    if (lu.tint.a > 0.0) {
        // screen, premultiplied: black leaves the layer as it is
        c = vec4<f32>(mix(c.rgb, c.rgb + lu.tint.rgb * (vec3<f32>(c.a) - c.rgb), lu.tint.a), c.a);
    }
    return c * lu.opacity;
}

// Normal layers: blended "over" by the pipeline
@fragment
fn fs_layer(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return layer_color(vec2<i32>(pos.xy));
}

// other modes: read what is below and write the result. In a clipping group a layer blends onto
// the base as if that were opaque and shows as much as the base does, the base's alpha kept (as
// alpha lock paints); the base itself is written as it is.
@fragment
fn fs_blend(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy);
    let c = layer_color(p);
    let b = textureLoad(backdrop_tex, p, 0);
    let atop = lu.clipped == CLIP_ATOP;
    var r = blend(select(b, vec4<f32>(unpremultiply(b), 1.0), atop), c, lu.blend);
    if (atop) {
        r = vec4<f32>(r.rgb * b.a, b.a);
    }
    return select(r, c, lu.clipped == CLIP_BASE);
}

// how high the paper stands at canvas point `at` (0 a hollow, 1 a peak): its tile repeated, each
// of its pixels lu.paper.y canvas pixels across
fn paper_height(at: vec2<f32>) -> f32 {
    let side = f32(textureDimensions(paper_tex).x);
    let level = max(log2(1.0 / lu.paper.y), 0.0);
    return textureSampleLevel(paper_tex, grain_smp, at / (lu.paper.y * side), level).r;
}

// the background colour (written, not blended); with a paper that shows in it, its hollows
// darker as deep as it says
@fragment
fn fs_solid(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    if (lu.paper.z > 0.0 && lu.solid.a > 0.0) {
        let at = pos.xy / lu.scaled.x + lu.scaled.yz;
        let shade = 1.0 - lu.paper.z * (1.0 - paper_height(at));
        return vec4<f32>(lu.solid.rgb * shade, lu.solid.a);
    }
    return lu.solid;
}

// the finished stroke at its opacity: blended "over" into the layer to paint, or as
// "destination out" to erase
@fragment
fn fs_commit(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return stroke_at(vec2<i32>(pos.xy));
}

// the finished stroke on an alpha-locked layer, or blended by a mode (written to a scratch
// texture, copied back)
@fragment
fn fs_commit_locked(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy);
    return with_stroke(textureLoad(layer_tex, p, 0), p);
}

// ---- blending -------------------------------------------------------------------------------


// premultiplied colour squared (nearer to light), and back
fn squared(c: vec4<f32>) -> vec4<f32> {
    let u = unpremultiply(c);
    return vec4<f32>(u * u * c.a, c.a);
}

fn unsquared(c: vec4<f32>) -> vec4<f32> {
    let u = unpremultiply(c);
    return vec4<f32>(sqrt(u) * c.a, c.a);
}

// the stroke `s` into the layer `b`: blended by its mode, and with luminance blending mixed in
// by its coverage in squared colour (the colour it lays down where it covers fully, mixed with
// the layer's). One way out, at the end: Samsung's Xclipse compiler, given a return part way
// with more after it, drew every pixel of an open stroke's layer wrong (black, or partly covered
// pixels solid: #153), whichever way the pixel went.
fn stroke_into(b: vec4<f32>, s: vec4<f32>) -> vec4<f32> {
    let mode = lu.stroke_blend & 255u;
    var into = blend(b, s, mode);
    if ((lu.stroke_blend & STROKE_LUMINANCE) != 0u && mode != 26u && mode != 29u && s.a > 0.0) {
        let cb = unpremultiply(b);
        let cs = unpremultiply(s);
        let mixed = clamp(blend_rgb(cb, cs, mode), vec3<f32>(0.0), vec3<f32>(1.0));
        let full = vec4<f32>((1.0 - b.a) * cs + b.a * mixed, 1.0);
        into = unsquared(mix(squared(b), squared(full), s.a));
    }
    return into;
}

