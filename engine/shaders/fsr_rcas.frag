#version 450

#define A_GPU 1
#define A_GLSL 1
#include "ffx_a.h"

#define FSR_RCAS_F 1

layout(set = 0, binding = 0) uniform sampler2D InputTexture;

vec4 FsrRcasLoadF(ASU2 p) { return texelFetch(InputTexture, ASU2(p), 0); }
void FsrRcasInputF(inout AF1 r, inout AF1 g, inout AF1 b) { }

#include "ffx_fsr1.h"

layout(push_constant) uniform PC {
    uvec4 con0;
} pc;

layout(location = 0) out vec4 outColor;

void main() {
    AU2 gxy = AU2(gl_FragCoord.xy);
    vec3 color;
    FsrRcasF(color.r, color.g, color.b, gxy, pc.con0);
    outColor = vec4(color, 1.0);
}
