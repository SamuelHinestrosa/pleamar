// Heat haze: what is behind the surface, bent by a slow shimmer. It keeps its
// alpha below 1, so the next capture can still see what is behind it.
fn shade(s: Shader) -> vec4<f32> {
    let wobble = vec2<f32>(sin(s.pos.y * 0.09 + s.time * 3.0), cos(s.pos.x * 0.07 + s.time * 2.3)) * 4.0 * s.a.x;
    let seen = behind(s, s.pos + wobble);
    // Where nothing is known of what is behind (the first frames, or a
    // compositor that does not say), the shimmer itself: rising bands of warm air.
    let rise = s.pos.y * 0.05 + s.time * 1.6 + sin(s.pos.x * 0.04 + s.time) * 1.5;
    let band = 0.5 + 0.5 * sin(rise + wobble.x * 0.4);
    let air = mix(s.color.rgb, vec3<f32>(0.62, 0.84, 0.74), band * 0.35 * (1.0 - s.uv.y * 0.6));
    let tone = mix(air, seen.rgb, seen.a);
    return vec4<f32>(tone, 0.88);
}
