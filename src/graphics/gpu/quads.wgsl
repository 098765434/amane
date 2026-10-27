// rounded rectangles, borders, letters, images and drop shadows, each drawn as one quad of two triangles

struct Quad {
    // where it is on the canvas, in pixels: x, y, width, height;
    // a shadow's is the rectangle grown by how far its blur reaches
    rect: vec4<f32>,

    // it only shows inside this rounded rectangle, and inside inner_clip too
    clip: vec4<f32>,

    // not premultiplied, 0 to 1
    color: vec4<f32>,

    // border thickness (0 fills it) or a shadow's gaussian deviation, two unused, and the kind:
    // 0 a shape, 1 a letter, 2 an image, 3 a shadow
    shape: vec4<f32>,

    // where it reads from, left, top, right, bottom: a letter's texels in the atlas,
    // or the part of the image, from 0 to 1; a shadow's rectangle, x, y, width, height
    source: vec4<f32>,

    inner_clip: vec4<f32>,

    // corner radii of the quad, its clip and its inner clip, clockwise from the top left
    radius: vec4<f32>,
    clip_radius: vec4<f32>,
    inner_radius: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> quads: array<Quad>;
@group(0) @binding(1) var<uniform> canvas: vec4<f32>;
@group(0) @binding(2) var atlas: texture_2d<f32>;

@group(1) @binding(0) var picture: texture_2d<f32>;
@group(1) @binding(1) var picture_sampler: sampler;

struct Corner {
    @builtin(position) position: vec4<f32>,
    @location(0) point: vec2<f32>,
    @location(1) source: vec2<f32>,
    @location(2) @interpolate(flat) index: u32,
}

@vertex
fn vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) index: u32) -> Corner {
    let quad = quads[index];

    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );

    let corner = corners[vertex];

    // a shape's soft edge reaches half a pixel past it; letters, images and shadows fill theirs exactly
    var grow = 1.0;

    if quad.shape.w != 0.0 {
        grow = 0.0;
    }

    let point = quad.rect.xy - grow + corner * (quad.rect.zw + grow * 2.0);

    let size = canvas.xy;

    var out: Corner;

    out.position = vec4<f32>(point.x / size.x * 2.0 - 1.0, 1.0 - point.y / size.y * 2.0, 0.0, 1.0);
    out.point = point;
    out.source = mix(quad.source.xy, quad.source.zw, corner);
    out.index = index;

    return out;
}

// how far point is outside the rounded rectangle, negative inside;
// radius holds the corners clockwise from the top left
fn rounded_rectangle(point: vec2<f32>, rect: vec4<f32>, radius: vec4<f32>) -> f32 {
    let half = rect.zw * 0.5;
    let center = rect.xy + half;

    let from_center = point - center;

    // the corner of the quarter the point is in, y grows downward
    let side = select(radius.xw, radius.yz, from_center.x > 0.0);
    let corner = select(side.x, side.y, from_center.y > 0.0);

    let safe_radius = clamp(corner, 0.0, min(half.x, half.y));

    let offset = abs(from_center) - half + safe_radius;

    let inside = min(max(offset.x, offset.y), 0.0);
    let outside = length(max(offset, vec2<f32>(0.0)));

    return inside + outside - safe_radius;
}

// one pixel of soft edge
fn coverage(distance: f32) -> f32 {
    return clamp(0.5 - distance, 0.0, 1.0);
}

// the error function, close enough for a blur; vello's erf7
fn erf7(x: f32) -> f32 {
    let y = clamp(x * 1.1283791671, -100.0, 100.0);
    let yy = y * y;
    let z = y + (0.24295 + (0.03395 + 0.0104 * yy) * yy) * (y * yy);

    return z / sqrt(1.0 + z * z);
}

fn hypot(a: f32, b: f32) -> f32 {
    return sqrt(a * a + b * b);
}

/*
 * how much of a gaussian-blurred rounded rectangle lands on point: the same
 * closed form vello's fine stage uses for its blurred rectangles, so a
 * shadow drawn here looks as it did there
 * (https://raphlinus.github.io/graphics/2020/04/21/blurred-rounded-rects.html)
 */
fn shadow(point: vec2<f32>, rect: vec4<f32>, radius: f32, deviation: f32) -> f32 {
    let std_dev = max(deviation, 1e-5);
    let inv_std_dev = 1.0 / std_dev;

    let min_edge = min(rect.z, rect.w);
    let radius_max = 0.5 * min_edge;
    let r0 = min(hypot(radius, std_dev * 1.15), radius_max);
    let r1 = min(hypot(radius, std_dev * 2.0), radius_max);

    let exponent = 2.0 * r1 / r0;
    let inv_exponent = 1.0 / exponent;

    // pull in the long end, so the blur looks less stretched
    let delta = 1.25 * std_dev * (exp(-pow(0.5 * inv_std_dev * rect.z, 2.0)) - exp(-pow(0.5 * inv_std_dev * rect.w, 2.0)));
    let width = rect.z + min(delta, 0.0);
    let height = rect.w - max(delta, 0.0);

    let scale = 0.5 * erf7(inv_std_dev * 0.5 * (max(width, height) - 0.5 * radius));

    let local = point - (rect.xy + rect.zw * 0.5);

    let y0 = abs(local.y) - (height * 0.5 - r1);
    let y1 = max(y0, 0.0);

    let x0 = abs(local.x) - (width * 0.5 - r1);
    let x1 = max(x0, 0.0);

    let d_pos = pow(pow(x1, exponent) + pow(y1, exponent), inv_exponent);
    let d_neg = min(max(x0, y0), 0.0);
    let d = d_pos + d_neg - r1;

    return scale * (erf7(inv_std_dev * (min_edge + d)) - erf7(inv_std_dev * d));
}

@fragment
fn fragment(corner: Corner) -> @location(0) vec4<f32> {
    let quad = quads[corner.index];

    // sampled for every quad, since sampling has to happen outside of branches
    let image = textureSample(picture, picture_sampler, corner.source);

    let outer_clip = coverage(rounded_rectangle(corner.point, quad.clip, quad.clip_radius));
    let inner_clip = coverage(rounded_rectangle(corner.point, quad.inner_clip, quad.inner_radius));

    let clip = outer_clip * inner_clip;

    // images hold premultiplied colors already, the color's alpha fades them
    if quad.shape.w == 2.0 {
        return image * quad.color.a * clip;
    }

    var amount = 0.0;

    if quad.shape.w == 1.0 {
        amount = textureLoad(atlas, vec2<i32>(floor(corner.source)), 0).r;
    } else if quad.shape.w == 3.0 {
        amount = shadow(corner.point, quad.source, quad.radius.x, quad.shape.x);
    } else {
        amount = coverage(rounded_rectangle(corner.point, quad.rect, quad.radius));

        let thickness = quad.shape.x;

        // a border is the shape minus the same shape pulled in by the thickness
        if thickness > 0.0 {
            let inner = vec4<f32>(quad.rect.xy + thickness, quad.rect.zw - thickness * 2.0);
            let inner_radius = max(quad.radius - thickness, vec4<f32>(0.0));

            var hole = 0.0;

            if inner.z > 0.0 && inner.w > 0.0 {
                hole = coverage(rounded_rectangle(corner.point, inner, inner_radius));
            }

            amount = amount * (1.0 - hole);
        }
    }

    amount = amount * clip;

    let alpha = quad.color.a * amount;

    return vec4<f32>(quad.color.rgb * alpha, alpha);
}
