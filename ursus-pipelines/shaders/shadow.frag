#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec2 fragUV;

layout(set = 0, binding = 0) uniform sampler   samp;
layout(set = 0, binding = 1) uniform texture2D textures[];

struct MaterialData {
    vec4  base_color;
    vec4  emissive;
    float metallic;
    float roughness;
    vec2  _pad;
    uvec4 tex_indices0;
    uvec4 tex_indices1;
};

layout(set = 1, binding = 0) readonly buffer MaterialBuffer {
    MaterialData materials[];
};

layout(push_constant) uniform PC {
    mat4 light_space_mvp;
    uint material_id;
} pc;

void main() {
    MaterialData mat = materials[pc.material_id];
    uint diffuse_idx = mat.tex_indices0.x;
    float alpha = texture(sampler2D(textures[nonuniformEXT(diffuse_idx)], samp), fragUV).a * mat.base_color.a;
    if (alpha < 0.5) discard;
}