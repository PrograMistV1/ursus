#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 fragNormal;
layout(location = 1) in vec2 fragUV;

layout(location = 0) out vec4 outAlbedo;
layout(location = 1) out vec4 outNormal;

layout(set = 0, binding = 0) uniform sampler   samp;
layout(set = 0, binding = 1) uniform texture2D textures[];

struct MaterialData {
    vec4  base_color;
    vec4  emissive;
    float metallic;
    float roughness;
    vec2  _pad;
    uvec4 tex_indices0;// diffuse, normal, metallic_roughness, emissive
    uvec4 tex_indices1;// occlusion, pad, pad, pad
};

layout(set = 1, binding = 0) readonly buffer MaterialBuffer {
    MaterialData materials[];
};

layout(push_constant) uniform PC {
    mat4 mvp;// offset   0
    mat4 model;// offset  64
    uint material_id;// offset 124
} pc;

void main() {
    MaterialData mat = materials[pc.material_id];
    uint diffuse_idx = mat.tex_indices0.x;
    vec4 tex_color   = texture(sampler2D(textures[nonuniformEXT(diffuse_idx)], samp), fragUV);

    outAlbedo = vec4(mat.base_color.rgb * tex_color.rgb, mat.base_color.a * tex_color.a);

    vec3 N = normalize(fragNormal);
    outNormal = vec4(N * 0.5 + 0.5, mat.roughness);
}