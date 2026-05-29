#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec2 inUV;

layout(push_constant) uniform PC {
    mat4 mvp;
    mat4 model;
} pc;

layout(location = 0) out vec3 fragNormal;
layout(location = 1) out vec2 fragUV;

void main() {
    fragNormal  = mat3(pc.model) * inNormal;
    fragUV      = inUV;
    gl_Position = pc.mvp * vec4(inPosition, 1.0);
}
