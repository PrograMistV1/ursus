use crate::assets::mesh::Vertex;
use crate::assets::{GpuMesh, ShaderRegistry};
use crate::ecs::components::Transform;
use crate::render_graph::GpuImage;
use crate::vulkan::pipeline::builder::{cmd, PipelineBuilder};
use crate::vulkan::resources::shadow_map::SHADOW_MAP_SIZE;
use ash::vk;
use glam::Mat4;

#[repr(C)]
pub struct ShadowPC {
    pub light_space_mvp: [[f32; 4]; 4],
}

pub struct ShadowDrawCall<'a> {
    pub gpu_mesh: &'a GpuMesh,
    pub transform: &'a Transform,
}

pub struct ShadowPass {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    device: ash::Device,
}

impl ShadowPass {
    pub fn new(device: &ash::Device, registry: &mut ShaderRegistry) -> anyhow::Result<Self> {
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(size_of::<ShadowPC>() as u32);

        let binding = vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);

        let attributes = [vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0)];

        let handle = registry.by_name("shadow").expect("шейдер 'shadow' не зарегистрирован");
        let (vert_spv, _) = registry.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();

        let (pipeline, layout) = PipelineBuilder::depth_only(&vert_spv, std::slice::from_ref(&binding), &attributes)
            .depth_bias(2.0, 1.5)
            .push_constants(std::slice::from_ref(&push_range))
            .build(device)?;

        log::debug!("ShadowPass создан");
        Ok(Self { pipeline, layout, device: device.clone() })
    }

    pub fn record(
        &self,
        device: &ash::Device,
        cmd_buf: vk::CommandBuffer,
        shadow_map: &impl GpuImage,
        light_view_proj: Mat4,
        draw_calls: &[ShadowDrawCall<'_>],
    ) {
        let extent = vk::Extent2D { width: SHADOW_MAP_SIZE, height: SHADOW_MAP_SIZE };

        cmd::begin_rendering_depth_only(device, cmd_buf, shadow_map.view(), extent);

        unsafe {
            device.cmd_bind_pipeline(cmd_buf, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

            for dc in draw_calls {
                let mvp = light_view_proj * dc.transform.matrix();
                let pc = ShadowPC { light_space_mvp: mvp.to_cols_array_2d() };
                let pc_bytes = std::slice::from_raw_parts(&pc as *const ShadowPC as *const u8, size_of::<ShadowPC>());
                device.cmd_push_constants(cmd_buf, self.layout, vk::ShaderStageFlags::VERTEX, 0, pc_bytes);
                device.cmd_bind_vertex_buffers(cmd_buf, 0, &[dc.gpu_mesh.vertex_buffer], &[0]);
                device.cmd_bind_index_buffer(cmd_buf, dc.gpu_mesh.index_buffer, 0, vk::IndexType::UINT32);
                device.cmd_draw_indexed(cmd_buf, dc.gpu_mesh.index_count, 1, 0, 0, 0);
            }

            device.cmd_end_rendering(cmd_buf);
        }
    }
}

impl Drop for ShadowPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}
