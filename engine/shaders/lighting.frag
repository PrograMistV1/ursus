#version 450

layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler2D gAlbedo;
layout(set = 0, binding = 1) uniform sampler2D gNormal;
layout(set = 0, binding = 2) uniform sampler2D gDepth;

struct DirectionalLight {
    vec4 direction;// xyz = dir, w = unused
    vec4 color;// rgb = color, a = intensity
};

struct PointLight {
    vec4 position;// xyz = pos, w = radius
    vec4 color;// rgb = color, a = intensity
};

layout(set = 0, binding = 3) uniform LightingUBO {
    DirectionalLight dir_light;
    PointLight point_lights[16];
    uint point_light_count;
} lights;

layout(push_constant) uniform PC {
    mat4 inv_proj;
    mat4 inv_view;
    vec2 viewport;
    vec2 _pad;
} pc;

const float AMBIENT = 0.04;

vec3 reconstruct_world_pos(vec2 uv, float depth) {
    vec4 ndc = vec4(uv * 2.0 - 1.0, depth, 1.0);
    vec4 view_pos = pc.inv_proj * ndc;
    view_pos /= view_pos.w;
    return (pc.inv_view * view_pos).xyz;
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

    vec3 albedo   = albedo_data.rgb;
    vec3 N        = normalize(normal_data.xyz * 2.0 - 1.0);
    float roughness = normal_data.a;
    vec3 world_pos  = reconstruct_world_pos(uv, depth);

    vec3 color = albedo * AMBIENT;

    // Directional light
    vec3 L_dir = normalize(-lights.dir_light.direction.xyz);
    float diff_dir = max(dot(N, L_dir), 0.0);
    color += albedo * diff_dir * lights.dir_light.color.rgb * lights.dir_light.color.a;

    // Point lights
    for (uint i = 0u; i < lights.point_light_count; i++) {
        vec3 to_light = lights.point_lights[i].position.xyz - world_pos;
        float dist    = length(to_light);
        float radius  = lights.point_lights[i].position.w;

        if (dist >= radius) continue;

        float attn = clamp(1.0 - (dist / radius), 0.0, 1.0);
        attn *= attn;

        float diff = max(dot(N, normalize(to_light)), 0.0);
        color += albedo * diff
        * lights.point_lights[i].color.rgb
        * lights.point_lights[i].color.a
        * attn;
    }

    outColor = vec4(color, albedo_data.a);
}