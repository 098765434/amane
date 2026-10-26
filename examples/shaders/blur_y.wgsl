// blurs input0 downward, values[0].x is how far in logical pixels; blur_x.wgsl is the same, sideways
@fragment
fn main(@location(0) uv: vec2<f32>, @location(1) canvas_uv: vec2<f32>) -> @location(0) vec4<f32> {
    return blurred(canvas_uv, vec2<f32>(0.0, 1.0 / placement.canvas.y));
}

fn weight(distance: f32, sigma: f32) -> f32 {
    return exp(-0.5 * (distance / sigma) * (distance / sigma));
}

// two neighbouring pixels are read at once, between them, with linear filtering
fn blurred(at: vec2<f32>, pixel: vec2<f32>) -> vec4<f32> {
    let sigma = max(values[0].x * scale, 0.001);
    let reach = min(i32(ceil(sigma * 3.0)), 64);

    var sum = textureSampleLevel(input0, input_sampler, at, 0.0);
    var total = 1.0;

    for (var near = 1; near <= reach; near += 2) {
        let first = weight(f32(near), sigma);
        let second = select(0.0, weight(f32(near + 1), sigma), near + 1 <= reach);
        let both = first + second;

        let offset = (f32(near) * first + f32(near + 1) * second) / both;

        sum += textureSampleLevel(input0, input_sampler, at + pixel * offset, 0.0) * both;
        sum += textureSampleLevel(input0, input_sampler, at - pixel * offset, 0.0) * both;

        total += both * 2.0;
    }

    return sum / total;
}
