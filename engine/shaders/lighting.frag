#version 450

layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler2D gAlbedo;
layout(set = 0, binding = 1) uniform sampler2D gNormal;
layout(set = 0, binding = 2) uniform sampler2D gDepth;

layout(push_constant) uniform PC {
    mat4  inv_proj;
    mat4  inv_view;
    vec4  light_pos;
    vec4  light_color;
    vec2  viewport;
    vec2  _pad;
} pc;

const float AMBIENT = 0.08;

vec3 reconstruct_world_pos(vec2 uv, float depth) {
    // NDC [-1..1]
    vec4 ndc = vec4(uv * 2.0 - 1.0, depth, 1.0);
    vec4 view_pos = pc.inv_proj * ndc;
    view_pos /= view_pos.w;
    vec4 world_pos = pc.inv_view * view_pos;
    return world_pos.xyz;
}

void main() {
    vec2 uv = gl_FragCoord.xy / pc.viewport;

    vec4 albedo_data = texture(gAlbedo, uv);
    vec4 normal_data = texture(gNormal, uv);
    float depth      = texture(gDepth, uv).r;

    if (depth >= 1.0) {
        outColor = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }

    vec3 albedo    = albedo_data.rgb;
    vec3 N         = normalize(normal_data.xyz * 2.0 - 1.0);
    float roughness = normal_data.a;

    vec3 world_pos = reconstruct_world_pos(uv, depth);

    vec3 L     = normalize(pc.light_pos.xyz - world_pos);
    float diff = max(dot(N, L), 0.0);

    float dist  = length(pc.light_pos.xyz - world_pos);
    float attn  = pc.light_color.w / (1.0 + 0.1 * dist * dist);

    vec3 color = albedo * (AMBIENT + diff * pc.light_color.rgb * attn);

    outColor = vec4(color, albedo_data.a);
}