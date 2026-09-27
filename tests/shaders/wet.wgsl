// What the group holds, bent by a ripple.
fn shade(s: Shader) -> vec4<f32> {
    let bent = inside(s, s.pos + vec2<f32>(sin(s.pos.y * 0.2 + s.time) * 3.0, 0.0));
    return vec4<f32>(bent.rgb, bent.a * s.a.x);
}
