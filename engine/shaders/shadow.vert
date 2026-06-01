#version 450

layout(location = 0) in vec3 inPosition;

layout(push_constant) uniform PC {
    mat4 light_space_mvp;
} pc;

void main() {
    gl_Position = pc.light_space_mvp * vec4(inPosition, 1.0);
}