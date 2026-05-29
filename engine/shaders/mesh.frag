#version 450

layout(location = 0) in vec3 fragNormal;
layout(location = 1) in vec2 fragUV;

layout(location = 0) out vec4 outColor;

layout(push_constant) uniform PC {
    mat4 mvp;
    mat4 model;
} pc;

const vec3  BASE_COLOR = vec3(0.8, 0.75, 0.65);
const vec3  LIGHT_DIR  = vec3(0.577, 0.577, 0.577);
const float AMBIENT    = 0.18;

void main() {
    vec3 N     = normalize(fragNormal);
    float diff = max(dot(N, LIGHT_DIR), 0.0);
    vec3  col  = BASE_COLOR * (AMBIENT + (1.0 - AMBIENT) * diff);
    outColor   = vec4(col, 1.0);
}
