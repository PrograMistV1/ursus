#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 fragTangent;
layout(location = 1) in vec3 fragBitangent;
layout(location = 2) in vec3 fragNormal;
layout(location = 3) in vec2 fragUV;

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
    mat4 mvp;
    mat4 model;
    uint material_id;
} pc;

void main() {
    MaterialData mat = materials[pc.material_id];

    uint diffuse_idx = mat.tex_indices0.x;
    vec4 tex_color   = texture(sampler2D(textures[nonuniformEXT(diffuse_idx)], samp), fragUV);
    vec4 albedo = vec4(mat.base_color.rgb * tex_color.rgb, mat.base_color.a * tex_color.a);

    if (albedo.a < 0.5) discard;
    outAlbedo = albedo;


    uint normal_idx = mat.tex_indices0.y;
    vec3 N;
    if (normal_idx != 0u) {
        vec3 n = texture(sampler2D(textures[nonuniformEXT(normal_idx)], samp), fragUV).rgb;
        n = n * 2.0 - 1.0;

        mat3 TBN = mat3(
        normalize(fragTangent),
        normalize(fragBitangent),
        normalize(fragNormal)
        );
        N = normalize(TBN * n);
    } else {
        N = normalize(fragNormal);
    }

    outNormal = vec4(N * 0.5 + 0.5, mat.roughness);
}