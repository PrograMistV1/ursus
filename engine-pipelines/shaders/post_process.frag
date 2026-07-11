#version 450

layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler2D hdrInput;

layout(push_constant) uniform PC {
    vec2  texel_size;
    float exposure;
    uint  flags;
} pc;

vec3 aces(vec3 x) {
    const float a = 2.51, b = 0.03, c = 2.43, d = 0.59, e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

void main() {
    vec2 uv = gl_FragCoord.xy * pc.texel_size;
    vec3 hdr = texture(hdrInput, uv).rgb;

    vec3 tonemapped = aces(hdr * pc.exposure);

    outColor = vec4(pow(tonemapped, vec3(1.0 / 2.2)), 1.0);
}
