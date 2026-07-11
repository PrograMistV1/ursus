#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 2) in vec2 inUV;

layout(push_constant) uniform PC {
    mat4 mvp;
    uint material_id;
} pc;

layout(location = 0) out vec2 fragUV;

void main() {
    gl_Position = pc.mvp * vec4(inPosition, 1.0);
    fragUV = inUV;
}