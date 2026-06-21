use crate::assets::ui::font_manager::GlyphInfo;
use crate::assets::ShaderRegistry;
use crate::vulkan::gfx_pipeline::builder::{cmd, PipelineBuilder};
use ash::vk;
use glam::Vec2;

#[repr(C)]
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
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    device: ash::Device,
}

impl UiPass {
    pub fn new(
        device: &ash::Device,
        swapchain_format: vk::Format,
        bindless_layout: vk::DescriptorSetLayout,
        registry: &mut ShaderRegistry,
    ) -> anyhow::Result<Self> {
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<UiPC>() as u32);

        let handle = registry.by_name("ui").expect("shader 'ui' not registered");
        let (vert_spv, frag_spv) = registry.load_spv(handle)?;
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

        let (pipeline, layout) =
            PipelineBuilder::fullscreen(&vert_spv, &frag_spv, std::slice::from_ref(&swapchain_format))
                .set_layouts(std::slice::from_ref(&bindless_layout))
                .push_constants(std::slice::from_ref(&push_range))
                .blend_attachments(&blend)
                .build(device)?;

        log::debug!("UiPass created");
        Ok(Self { pipeline, layout, device: device.clone() })
    }

    pub fn begin(
        &self,
        device: &ash::Device,
        cmd_buf: vk::CommandBuffer,
        swapchain_view: vk::ImageView,
        extent: vk::Extent2D,
        bindless_set: vk::DescriptorSet,
    ) {
        cmd::begin_rendering_load(device, cmd_buf, swapchain_view, extent);
        unsafe {
            device.cmd_bind_pipeline(cmd_buf, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                &[bindless_set],
                &[],
            );
        }
    }

    pub fn end(&self, device: &ash::Device, cmd_buf: vk::CommandBuffer) {
        unsafe { device.cmd_end_rendering(cmd_buf) };
    }

    pub fn draw_rect(
        &self,
        device: &ash::Device,
        cmd_buf: vk::CommandBuffer,
        screen_size: [f32; 2],
        pos: Vec2,
        size: Vec2,
        color: [f32; 4],
    ) {
        let pc = UiPC {
            screen_size,
            pos: pos.into(),
            size: size.into(),
            _pad0: [0.0; 2],
            color,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tex_index: 0,
            use_texture: 0,
            sdf_mode: 0,
            _pad1: 0,
        };
        self.push_and_draw(device, cmd_buf, &pc);
    }

    pub fn draw_sdf_glyph(
        &self,
        device: &ash::Device,
        cmd_buf: vk::CommandBuffer,
        screen_size: [f32; 2],
        origin: Vec2,
        glyph: &GlyphInfo,
        bindless_slot: u32,
        color: [f32; 4],
    ) {
        if glyph.width == 0 || glyph.height == 0 {
            return;
        }

        let x = origin.x + glyph.offset_x as f32;

        let y = origin.y - (glyph.offset_y as f32 + glyph.height as f32);

        let pc = UiPC {
            screen_size,
            pos: [x, y],
            size: [glyph.width as f32, glyph.height as f32],
            _pad0: [0.0; 2],
            color,
            uv_rect: [glyph.u0, glyph.v0, glyph.u1, glyph.v1],
            tex_index: bindless_slot,
            use_texture: 1,
            sdf_mode: 1,
            _pad1: 0,
        };
        self.push_and_draw(device, cmd_buf, &pc);
    }

    fn push_and_draw(&self, device: &ash::Device, cmd_buf: vk::CommandBuffer, pc: &UiPC) {
        unsafe {
            let bytes = std::slice::from_raw_parts(pc as *const UiPC as *const u8, size_of::<UiPC>());
            device.cmd_push_constants(
                cmd_buf,
                self.layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                bytes,
            );
            device.cmd_draw(cmd_buf, 6, 1, 0, 0);
        }
    }
}

impl Drop for UiPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}
