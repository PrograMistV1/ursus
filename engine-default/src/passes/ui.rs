use ash::vk;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::gfx::format::Format;
use engine_core::render::gfx::{CommandEncoder, PipelineId, ShaderStage};
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{PreparedUiDrawList, RenderWorld, UiPrimitive};
use glam::Vec2;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiPC {
    pub screen_size: [f32; 2],
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub _pad0: [f32; 2],
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],
    pub tex_index: u32,
    pub use_texture: u32,
    pub sdf_mode: u32,
    pub _pad1: u32,
}

pub struct UiPass {
    pipeline: PipelineId,
}

impl UiPass {
    pub fn new(gpu: &mut GpuAssetServer, swapchain_format: Format) -> anyhow::Result<Self> {
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<UiPC>() as u32);

        let handle = gpu.shaders.by_name("ui").expect("shader 'ui' not registered");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'ui' must have frag").to_vec();

        let blend = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA)];

        let bindless_layout = gpu.bindless.layout;

        let pipeline = gpu.create_fullscreen_pipeline(
            &vert_spv,
            &frag_spv,
            std::slice::from_ref(&swapchain_format),
            std::slice::from_ref(&bindless_layout),
            std::slice::from_ref(&push_range),
            Some(&blend),
        )?;

        Ok(Self { pipeline })
    }

    pub fn record(
        &self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        gpu: &GpuAssetServer,
        swapchain: ResourceHandle,
    ) -> anyhow::Result<()> {
        let Some(draw_list) = rw.get::<PreparedUiDrawList>() else {
            return Ok(());
        };
        self.record_draw_list(enc, draw_list, gpu, swapchain)
    }

    pub fn record_draw_list(
        &self,
        enc: &mut CommandEncoder,
        draw_list: &PreparedUiDrawList,
        gpu: &GpuAssetServer,
        swapchain: ResourceHandle,
    ) -> anyhow::Result<()> {
        let screen = enc.extent_of(swapchain);

        enc.begin_rendering_load(swapchain);
        enc.bind_pipeline(self.pipeline);
        enc.bind_descriptor_sets(self.pipeline, &[gpu.bindless.set]);

        for primitive in &draw_list.primitives {
            let pc = match primitive {
                UiPrimitive::Rect { pos, size, color, .. } => {
                    self.make_pc(screen, *pos, *size, *color, [0.0, 0.0, 1.0, 1.0], 0, 0)
                }
                UiPrimitive::TexturedRect { pos, size, color, bindless_slot, uv } => {
                    self.make_pc(screen, *pos, *size, *color, *uv, *bindless_slot, 2)
                }
                UiPrimitive::GlyphRect { pos, size, color, texture_handle, uv } => {
                    let slot = gpu.texture_slot(*texture_handle);
                    self.make_pc(screen, *pos, *size, *color, *uv, slot, 1)
                }
            };
            enc.push_constants(self.pipeline, ShaderStage::VertexFragment, &pc);
            enc.draw(6);
        }

        enc.end_rendering();
        Ok(())
    }

    fn make_pc(
        &self,
        screen: [f32; 2],
        pos: Vec2,
        size: Vec2,
        color: [f32; 4],
        uv: [f32; 4],
        tex_index: u32,
        use_texture: u32,
    ) -> UiPC {
        UiPC {
            screen_size: screen,
            pos: pos.into(),
            size: size.into(),
            _pad0: [0.0; 2],
            color,
            uv_rect: uv,
            tex_index,
            use_texture,
            sdf_mode: 0,
            _pad1: 0,
        }
    }
}
