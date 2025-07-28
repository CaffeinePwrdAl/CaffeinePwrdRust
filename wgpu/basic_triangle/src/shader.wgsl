struct Transforms {
    vp: mat4x4<f32>,
    m: mat4x4<f32>,
};

@group(0)
@binding(0)
var<uniform> xforms: Transforms;

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
    result.position = xforms.vp * xforms.m * position;
    return result;
}

fn pal(t: f32, a: vec3f, b: vec3f, c: vec3f, d: vec3f ) -> vec3f
{
    return a + b*cos( 6.28318*(c*t+d) );
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    
    let t = vertex.uv.x + vertex.uv.y;

    // http://iquilezles.org/articles/palettes
    // Copyright © 2015 Inigo Quilez
    let col = pal( t, vec3(0.5,0.5,0.5),vec3(0.5,0.5,0.5),vec3(1.0,1.0,1.0),vec3(0.0,0.33,0.67) );

    return vec4f(col*col, 1.0);
}