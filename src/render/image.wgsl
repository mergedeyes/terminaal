struct ScreenUniform {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

@group(1) @binding(0)
var image: texture_2d<f32>;
@group(1) @binding(1)
var image_sampler: sampler;

struct VertexInput {
    @location(0) unit_pos: vec2<f32>,
};

struct InstanceInput {
    @location(1) offset: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) uv_offset: vec2<f32>,
    @location(4) uv_size: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let pixel_pos = instance.offset + vertex.unit_pos * instance.size;
    let ndc_x = (pixel_pos.x / screen.size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / screen.size.y) * 2.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = instance.uv_offset + vertex.unit_pos * instance.uv_size;
    return out;
}

// The texture holds straight alpha (sRGB, so sampling gives linear);
// everything is drawn premultiplied.
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(image, image_sampler, in.uv);
    return vec4<f32>(color.rgb * color.a, color.a);
}
