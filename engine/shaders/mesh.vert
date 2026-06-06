#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec2 inUV;
layout(location = 3) in vec4 inTangent;

layout(push_constant) uniform PC {
    mat4 mvp;
    mat4 model;
} pc;

layout(location = 0) out vec3 fragTangent;
layout(location = 1) out vec3 fragBitangent;
layout(location = 2) out vec3 fragNormal;
layout(location = 3) out vec2 fragUV;

void main() {
    mat3 normalMatrix = mat3(pc.model);

    vec3 N = normalize(normalMatrix * inNormal);
    vec3 T = normalize(normalMatrix * inTangent.xyz);
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T) * inTangent.w;

    fragTangent   = T;
    fragBitangent = B;
    fragNormal    = N;
    fragUV        = inUV;

    gl_Position = pc.mvp * vec4(inPosition, 1.0);
}
