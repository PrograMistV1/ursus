#version 450

#define A_GPU 1
#define A_GLSL 1
#include "ffx_a.h"

#define FSR_EASU_F 1

layout(set = 0, binding = 0) uniform sampler2D InputTexture;

AF4 FsrEasuRF(AF2 p) { return AF4(textureGather(InputTexture, p, 0)); }
AF4 FsrEasuGF(AF2 p) { return AF4(textureGather(InputTexture, p, 1)); }
AF4 FsrEasuBF(AF2 p) { return AF4(textureGather(InputTexture, p, 2)); }

#include "ffx_fsr1.h"

layout(push_constant) uniform PC {
    uvec4 con0;
    uvec4 con1;
    uvec4 con2;
    uvec4 con3;
} pc;

layout(location = 0) out vec4 outColor;

void main() {
    AU2 gxy = AU2(gl_FragCoord.xy);
    vec3 color;
    FsrEasuF(color, gxy, pc.con0, pc.con1, pc.con2, pc.con3);
    outColor = vec4(color, 1.0);
}
