#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec2 fragUV;
layout(location = 1) in vec4 fragColor;
layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler   samp;
layout(set = 0, binding = 1) uniform texture2D textures[];

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

void main() {
    if (pc.use_texture != 0u) {
        float a = texture(sampler2D(textures[nonuniformEXT(pc.tex_index)], samp), fragUV).a;
        outColor = vec4(fragColor.rgb, fragColor.a * a);
    } else {
        outColor = fragColor;
    }
}