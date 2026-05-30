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
    return clamp((x*(a*x+b))/(x*(c*x+d)+e), 0.0, 1.0);
}

float luma(vec3 c) { return dot(c, vec3(0.299, 0.587, 0.114)); }

vec3 fxaa(sampler2D tex, vec2 uv, vec2 texel) {
    vec3 rgbM  = texture(tex, uv).rgb;
    vec3 rgbNW = texture(tex, uv + vec2(-1, -1)*texel).rgb;
    vec3 rgbNE = texture(tex, uv + vec2(1, -1)*texel).rgb;
    vec3 rgbSW = texture(tex, uv + vec2(-1, 1)*texel).rgb;
    vec3 rgbSE = texture(tex, uv + vec2(1, 1)*texel).rgb;

    float lumM  = luma(rgbM);
    float lumNW = luma(rgbNW);
    float lumNE = luma(rgbNE);
    float lumSW = luma(rgbSW);
    float lumSE = luma(rgbSE);

    float lumMin = min(lumM, min(min(lumNW, lumNE), min(lumSW, lumSE)));
    float lumMax = max(lumM, max(max(lumNW, lumNE), max(lumSW, lumSE)));
    float range  = lumMax - lumMin;

    if (range < max(0.0833, lumMax * 0.166)) return rgbM;

    vec2 dir = vec2(
    -((lumNW+lumNE)-(lumSW+lumSE)),
    ((lumNW+lumSW)-(lumNE+lumSE))
    );
    float dirReduce = max((lumNW+lumNE+lumSW+lumSE)*0.03125, 0.0078125);
    float rcpDirMin = 1.0 / (min(abs(dir.x), abs(dir.y)) + dirReduce);
    dir = clamp(dir * rcpDirMin, -8.0, 8.0) * texel;

    vec3 rgbA = 0.5 * (
    texture(tex, uv + dir * (1.0/3.0 - 0.5)).rgb +
    texture(tex, uv + dir * (2.0/3.0 - 0.5)).rgb);
    vec3 rgbB = rgbA * 0.5 + 0.25 * (
    texture(tex, uv + dir * -0.5).rgb +
    texture(tex, uv + dir *  0.5).rgb);

    return (luma(rgbB) < lumMin || luma(rgbB) > lumMax) ? rgbA : rgbB;
}

void main() {
    vec2 uv = gl_FragCoord.xy * pc.texel_size;

    vec3 hdr = (pc.flags & 1u) != 0u
    ? fxaa(hdrInput, uv, pc.texel_size)
    : texture(hdrInput, uv).rgb;

    vec3 tonemapped = hdr * pc.exposure;//aces(hdr * pc.exposure);

    outColor = vec4(pow(tonemapped, vec3(1.0/2.2)), 1.0);
}
