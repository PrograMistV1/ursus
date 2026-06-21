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
    uint sdf_mode;
    uint _pad1;
} pc;

const float SDF_PX_RANGE = 4.0;

void main() {
    if (pc.use_texture == 0u) {
        outColor = fragColor;
        return;
    }

    if (pc.sdf_mode != 0u) {
        float sdfv = texture(sampler2D(textures[nonuniformEXT(pc.tex_index)], samp), fragUV).r;

        vec2 tex_size = vec2(textureSize(sampler2D(textures[nonuniformEXT(pc.tex_index)], samp), 0));
        vec2 unit_range = vec2(SDF_PX_RANGE) / tex_size;
        vec2 screen_tex_size = vec2(1.0) / fwidth(fragUV);
        float range = max(0.5 * dot(unit_range, screen_tex_size), 1.0);

        float dist = range * (sdfv - 0.5);
        float a = clamp(dist + 0.5, 0.0, 1.0);
        outColor = vec4(fragColor.rgb, fragColor.a * a);
    } else {
        float a = texture(sampler2D(textures[nonuniformEXT(pc.tex_index)], samp), fragUV).a;
        outColor = vec4(fragColor.rgb, fragColor.a * a);
    }
}