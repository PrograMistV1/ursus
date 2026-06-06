use ash::vk;

pub struct UiPass;

impl UiPass {
    pub fn record(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        swapchain_view: vk::ImageView,
        extent: vk::Extent2D,
        window: &winit::window::Window,
        egui: &mut crate::egui_layer::EguiLayer,
        egui_output: egui::FullOutput,
        graphics_queue: vk::Queue,
        command_pool: vk::CommandPool,
    ) -> anyhow::Result<()> {
        unsafe {
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);

            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&color_attachment)),
            );

            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            device.cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent }]);
        }

        egui.end_frame_and_draw(window, graphics_queue, command_pool, cmd, extent, egui_output)?;

        unsafe {
            device.cmd_end_rendering(cmd);
        }

        Ok(())
    }
}
