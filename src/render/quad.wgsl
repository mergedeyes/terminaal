struct ScreenUniform {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

struct VertexInput {
    @location(0) unit_pos: vec2<f32>,
};

struct InstanceInput {
    @location(1) offset: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let pixel_pos = instance.offset + vertex.unit_pos * instance.size;
    let ndc_x = (pixel_pos.x / screen.size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / screen.size.y) * 2.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
