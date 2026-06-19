#version 450

layout(push_constant) uniform PC {
    vec2 screen_size;
    vec2 pos;
    vec2 size;
    vec2 _pad0;
    vec4 color;
    vec4 uv_rect;
    uint tex_index;
    uint use_texture;
    vec2 _pad1;
} pc;

layout(location = 0) out vec2 fragUV;
layout(location = 1) out vec4 fragColor;

const vec2 unit_quad[6] = vec2[](
vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
);

void main() {
    vec2 local = unit_quad[gl_VertexIndex];
    vec2 pix = pc.pos + local * pc.size;
    vec2 ndc = (pix / pc.screen_size) * 2.0 - 1.0;
    gl_Position = vec4(ndc, 0.0, 1.0);
    fragUV = mix(pc.uv_rect.xy, pc.uv_rect.zw, local);
    fragColor = pc.color;
}