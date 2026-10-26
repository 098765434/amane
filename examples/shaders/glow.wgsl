// lays input0, the drawing, over input1, its blur, tinted by values[1]
@fragment
fn main(@location(0) uv: vec2<f32>, @location(1) canvas_uv: vec2<f32>) -> @location(0) vec4<f32> {
    // inputs hold premultiplied colors, what's returned is plain
    let drawing = textureSampleLevel(input0, input_sampler, canvas_uv, 0.0);
    let blurred = textureSampleLevel(input1, input_sampler, canvas_uv, 0.0);

    let tint = values[1];
    let glow = blurred.a * tint.a * (1.0 - drawing.a);

    let alpha = drawing.a + glow;
    let color = drawing.rgb + tint.rgb * glow;

    return vec4<f32>(color / max(alpha, 0.0001), alpha);
}
