pub const MAX_POINT_LIGHTS: usize = 16;

// TODO: DirectionalLightComponent/PointLightComponent (ecs/components/light.rs),
// LightExtract (render/extract/lights.rs), DirectionalLight/GpuPointLight/ExtractedLights
// (render/gfx/types/light.rs, render/world.rs) are a concrete lighting implementation
// for a single pipeline (shadow mapping for directional lights), not a general
// core plumbing abstraction
// (see README: "engine-core has no opinion about how you render things").
// LightExtract is currently hardcoded into ExtractSchedule::default(), causing it
// to run even for pipelines without a scene (LoadingPipeline). Move it to
// engine-pipelines once a plugin/extension system for ExtractSchedule exists -
// App/RenderPipeline should register the extract systems they need via the existing
// ExtractSchedule::add(), rather than getting them for free from Default.

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DirectionalLight {
    pub direction: [f32; 4],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GpuPointLight {
    pub position: [f32; 4], // xyz = pos, w = radius
    pub color: [f32; 4],    // rgb = color, a = intensity
}
