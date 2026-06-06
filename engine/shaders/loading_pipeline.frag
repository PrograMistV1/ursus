#version 450

layout(location = 0) out vec4 outColor;

layout(push_constant) uniform PC {
    float time;
    float progress;
    float width;
    float height;
} pc;

void main() {
    vec2 uv = gl_FragCoord.xy / vec2(pc.width, pc.height);

    // Тёмный градиент фон
    vec3 top    = vec3(0.08, 0.08, 0.12);
    vec3 bottom = vec3(0.04, 0.04, 0.06);
    vec3 bg = mix(top, bottom, uv.y);

    // Прогресс-полоска внизу (3px)
    float bar_h = 3.0 / pc.height;
    float bar_y = 1.0 - bar_h;

    if (uv.y >= bar_y) {
        float filled = step(uv.x, pc.progress);
        float pulse  = 0.5 + 0.5 * sin(pc.time * 3.0 - uv.x * 6.0);
        vec3 accent  = mix(vec3(0.2, 0.5, 1.0), vec3(0.4, 0.7, 1.0), pulse);
        vec3 track   = vec3(0.15, 0.15, 0.2);
        outColor = vec4(mix(track, accent, filled), 1.0);
        return;
    }

    outColor = vec4(bg, 1.0);
}
