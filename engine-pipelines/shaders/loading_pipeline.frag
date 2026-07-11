#version 450

layout(location = 0) out vec4 outColor;

layout(push_constant) uniform PC {
    float time;
    float progress;
    float width;
    float height;
} pc;

void main() {
    vec3 bg = vec3(0.05, 0.05, 0.05);
    outColor = vec4(bg, 1.0);
}
