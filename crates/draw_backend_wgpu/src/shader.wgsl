// Backend-neutral `DrawList` -> GPU shader.
//
// Geometry is tessellated on the CPU into triangles whose positions are already
// in normalized device coordinates (NDC). Transform, opacity and clip are
// therefore resolved before the GPU sees the vertices, so this shader stays a
// trivial textured-quad pass. `uv` addresses a 1x1 white texture for solid
// fills, a registered image, or the built-in bitmap-font atlas.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(u_texture, u_sampler, input.uv);
    return texel * input.color;
}

// Post-process effects, selected per texture by `TextureEffect`. They read the
// source texel grid via `textureDimensions`, so a low-resolution canvas gets a
// look that follows its own pixels rather than the screen's.

/// Darken alternate source rows.
fn scanline(input: VertexOutput, dims: vec2<f32>) -> f32 {
    let row = floor(input.uv.y * dims.y);
    return select(1.0, 0.55, (row % 2.0) < 1.0);
}

@fragment
fn fs_scanlines(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(u_texture, u_sampler, input.uv);
    let dims = vec2<f32>(textureDimensions(u_texture, 0));
    let factor = scanline(input, dims);
    return vec4<f32>(texel.rgb * factor, texel.a) * input.color;
}

@fragment
fn fs_lcd(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(u_texture, u_sampler, input.uv);
    let dims = vec2<f32>(textureDimensions(u_texture, 0));
    let column = floor(input.uv.x * dims.x);
    let row = floor(input.uv.y * dims.y);
    let on_edge = (column % 2.0) < 1.0 || (row % 2.0) < 1.0;
    let grid = select(1.0, 0.62, on_edge);
    return vec4<f32>(texel.rgb * grid, texel.a) * input.color;
}

@fragment
fn fs_crt(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(u_texture, u_sampler, input.uv);
    let dims = vec2<f32>(textureDimensions(u_texture, 0));
    let factor = scanline(input, dims);
    // Aperture grille: bias one channel per column triad.
    let column = floor(input.uv.x * dims.x * 3.0) % 3.0;
    var mask = vec3<f32>(1.15, 0.9, 0.9);
    if (column >= 1.0 && column < 2.0) {
        mask = vec3<f32>(0.9, 1.15, 0.9);
    } else if (column >= 2.0) {
        mask = vec3<f32>(0.9, 0.9, 1.15);
    }
    let rgb = clamp(texel.rgb * mask * factor, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(rgb, texel.a) * input.color;
}

@fragment
fn fs_sharpen(input: VertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(u_texture, 0));
    let step = 1.0 / max(dims, vec2<f32>(1.0));
    let center = textureSample(u_texture, u_sampler, input.uv);
    let left = textureSample(u_texture, u_sampler, input.uv - vec2<f32>(step.x, 0.0));
    let right = textureSample(u_texture, u_sampler, input.uv + vec2<f32>(step.x, 0.0));
    let up = textureSample(u_texture, u_sampler, input.uv - vec2<f32>(0.0, step.y));
    let down = textureSample(u_texture, u_sampler, input.uv + vec2<f32>(0.0, step.y));
    let sharp = center * 5.0 - (left + right + up + down);
    let rgb = clamp(sharp.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(rgb, center.a) * input.color;
}
