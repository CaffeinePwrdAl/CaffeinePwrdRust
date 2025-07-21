
@group(0)
@binding(0)
var<uniform> transform: mat4x4<f32>;

struct VertexOutput {
    @location(0) uv: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec4<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOutput {
    var result: VertexOutput;
    result.uv = uv;
    result.position = transform * position;
    return result;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    
    let t = length(vertex.uv);

    let a = vec4f(0.3, 0.5, 0.7, 1.0);
    let b = vec4f(0.12, 0.24, 0.1, 0.0);
    let c = vec4f(0.12, 0.24, 0.1, 0.0);
    let d = vec4f(0.4, 0.9, 0.85, 0.0);

    return vec4f(a + b * cos(c * t));
}