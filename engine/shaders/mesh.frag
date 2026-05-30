#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 fragNormal;
layout(location = 1) in vec2 fragUV;

layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler   samp;
layout(set = 0, binding = 1) uniform texture2D textures[];

struct MaterialData {
    vec4  base_color;
    vec4  emissive;
    float metallic;
    float roughness;
    vec2  _pad;
    uvec4 tex_indices0;// diffuse, normal, metallic_roughness, emissive
    uint  tex_occlusion;
    uvec3 _pad2;
};
layout(set = 1, binding = 0) readonly buffer MaterialBuffer {
    MaterialData materials[];
};

layout(push_constant) uniform PC {
    mat4 mvp;// offset   0
    mat4 model;// offset  64
    uint material_id;// offset 124
} pc;

const vec3  LIGHT_DIR = normalize(vec3(1.0, 2.0, 1.5));
const float AMBIENT   = 0.15;

void main() {
    MaterialData mat = materials[pc.material_id];

    uint diffuse_idx = mat.tex_indices0.x;
    vec4 tex_color   = texture(sampler2D(textures[nonuniformEXT(diffuse_idx)], samp), fragUV);

    vec3 base = mat.base_color.rgb * tex_color.rgb;

    vec3 N    = normalize(fragNormal);
    float diff = max(dot(N, LIGHT_DIR), 0.0);
    vec3  col  = base * (AMBIENT + (1.0 - AMBIENT) * diff);

    uint emissive_idx = mat.tex_indices0.w;
    if (emissive_idx != 0u) {
        vec3 emissive_tex = texture(sampler2D(textures[nonuniformEXT(emissive_idx)], samp), fragUV).rgb;
        col += mat.emissive.rgb * emissive_tex;
    }

    outColor = vec4(col, mat.base_color.a * tex_color.a);
}