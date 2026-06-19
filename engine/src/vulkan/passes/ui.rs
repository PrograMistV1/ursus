use crate::assets::ui::FontAtlas;
use crate::assets::ShaderRegistry;
use crate::vulkan::pipeline::builder::{cmd, PipelineBuilder};
use ash::vk;
use glam::Vec2;

#[repr(C)]
struct UiPC {
    screen_size: [f32; 2],
    pos: [f32; 2],
    size: [f32; 2],
    _pad0: [f32; 2],
    color: [f32; 4],
    uv_rect: [f32; 4],
    tex_index: u32,
    use_texture: u32,
    _pad1: [f32; 2],
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

        let handle = registry.by_name("ui").expect("шейдер 'ui' не зарегистрирован");
        let (vert_spv, frag_spv) = registry.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'ui' должен иметь frag").to_vec();

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

        log::debug!("UiPass создан");
        Ok(Self { pipeline, layout, device: device.clone() })
    }

    pub fn record(
        &self,
        device: &ash::Device,
        cmd_buf: vk::CommandBuffer,
        swapchain_view: vk::ImageView,
        extent: vk::Extent2D,
        bindless_set: vk::DescriptorSet,
        rects: &[(Vec2, Vec2, [f32; 4])],
        texts: &[(Vec2, String, f32, [f32; 4])],
        font_atlas: Option<&FontAtlas>,
        font_atlas_tex: u32,
    ) -> anyhow::Result<()> {
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

        let screen_size = [extent.width as f32, extent.height as f32];

        for (pos, size, color) in rects {
            let pc = UiPC {
                screen_size,
                pos: [pos.x, pos.y],
                size: [size.x, size.y],
                _pad0: [0.0; 2],
                color: *color,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tex_index: 0,
                use_texture: 0,
                _pad1: [0.0; 2],
            };
            self.draw_quad(device, cmd_buf, &pc);
        }

        if let Some(atlas) = font_atlas {
            for (origin, text, font_size, color) in texts {
                let font_size = *font_size as u32;
                let line_height = atlas.line_height(font_size);

                let mut cursor_x = origin.x;
                let origin_y = origin.y;

                for ch in text.chars() {
                    let advance = atlas.get_advance(ch, font_size);
                    if let Some(glyph) = atlas.get_glyph(ch, font_size) {
                        if glyph.width > 0 && glyph.height > 0 {
                            let gx = cursor_x + glyph.offset_x as f32;
                            let gy = origin_y + line_height - glyph.height as f32 - glyph.offset_y as f32;

                            let pc = UiPC {
                                screen_size,
                                pos: [gx, gy],
                                size: [glyph.width as f32, glyph.height as f32],
                                _pad0: [0.0; 2],
                                color: *color,
                                uv_rect: [glyph.u0, glyph.v0, glyph.u1, glyph.v1],
                                tex_index: font_atlas_tex,
                                use_texture: 1,
                                _pad1: [0.0; 2],
                            };
                            self.draw_quad(device, cmd_buf, &pc);
                        }
                    }
                    cursor_x += advance;
                }
            }
        }

        unsafe { device.cmd_end_rendering(cmd_buf) };
        Ok(())
    }

    fn draw_quad(&self, device: &ash::Device, cmd_buf: vk::CommandBuffer, pc: &UiPC) {
        unsafe {
            let pc_bytes = std::slice::from_raw_parts(pc as *const UiPC as *const u8, size_of::<UiPC>());
            device.cmd_push_constants(
                cmd_buf,
                self.layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                pc_bytes,
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
