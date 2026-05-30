use super::material_buffer::MaterialBuffer;
use super::{commands::Commands, sync::FrameSync, Device, Pipeline, VulkanContext};
use crate::assets::material::MaterialData;
use crate::assets::{AssetServer, GpuMesh};
use crate::ecs::components::{MaterialHandle, Transform};
use ash::vk;
use glam::{Mat4, Vec3};
use std::sync::Arc;

const FRAMES_IN_FLIGHT: u32 = 2;

pub struct DrawCall<'a> {
    pub gpu_mesh: &'a GpuMesh,
    pub transform: &'a Transform,
    pub material: Option<MaterialHandle>,
}

pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(2.0, 2.0, 3.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: 60_f32.to_radians(),
            z_near: 0.1,
            z_far: 100.0,
        }
    }
}

impl Camera {
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let mut proj = Mat4::perspective_rh(self.fov_y, aspect, self.z_near, self.z_far);
        proj.y_axis.y *= -1.0; // Vulkan: Y вниз
        proj * view
    }
}

#[repr(C)]
struct MeshPushConstants {
    mvp: [[f32; 4]; 4],   // 64 B
    model: [[f32; 4]; 4], // 60 B
    material_id: u32,
}

pub struct Renderer {
    pub mesh_pipeline: Pipeline,
    pub commands: Commands,
    pub material_buffer: MaterialBuffer,
    frames: Vec<FrameSync>,
    current_frame: usize,
    swapchain_loader: ash::khr::swapchain::Device,
    device: Arc<Device>,
}

impl Renderer {
    pub fn new(ctx: &VulkanContext, assets: &AssetServer) -> anyhow::Result<Self> {
        let swapchain = ctx.swapchain.as_ref().unwrap();

        let material_buffer = MaterialBuffer::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
        )?;

        let mesh_pipeline = Pipeline::new_mesh(
            &ctx.device.handle,
            swapchain.format,
            assets.bindless.layout,
            material_buffer.layout,
        )?;

        let frames: anyhow::Result<Vec<_>> = (0..FRAMES_IN_FLIGHT)
            .map(|_| FrameSync::new(&ctx.device.handle))
            .collect();
        let frames = frames?;

        let commands = Commands::new(
            &ctx.device.handle,
            ctx.device.graphics_family,
            FRAMES_IN_FLIGHT,
        )?;

        let swapchain_loader =
            ash::khr::swapchain::Device::new(&ctx.instance.handle, &ctx.device.handle);

        Ok(Self {
            mesh_pipeline,
            commands,
            material_buffer,
            frames,
            current_frame: 0,
            swapchain_loader,
            device: ctx.device.clone(),
        })
    }

    pub fn draw_frame(
        &mut self,
        ctx: &VulkanContext,
        clear_color: [f32; 4],
        camera: &Camera,
        draw_calls: &[DrawCall<'_>],
        assets: &AssetServer,
    ) -> anyhow::Result<()> {
        let frame = &self.frames[self.current_frame];
        let cmd = self.commands.buffers[self.current_frame];
        let device = &ctx.device.handle;
        let swapchain = ctx.swapchain.as_ref().unwrap();
        let aspect = swapchain.extent.width as f32 / swapchain.extent.height as f32;
        let view_proj = camera.view_proj(aspect);

        let material_data: Vec<MaterialData> = draw_calls
            .iter()
            .map(|dc| {
                dc.material
                    .and_then(|mh| assets.get_material(mh))
                    .map(|m| m.to_gpu_data())
                    .unwrap_or_else(MaterialData::default_white)
            })
            .collect();

        self.material_buffer.upload(&material_data);

        unsafe {
            device.wait_for_fences(&[frame.render_fence], true, u64::MAX)?;
            device.reset_fences(&[frame.render_fence])?;
        }

        let (image_index, _suboptimal) = unsafe {
            self.swapchain_loader.acquire_next_image(
                swapchain.handle,
                u64::MAX,
                frame.image_available,
                vk::Fence::null(),
            )?
        };

        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;

            Self::transition_image(
                device,
                cmd,
                swapchain.images[image_index as usize],
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            );

            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(swapchain.image_views[image_index as usize])
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: clear_color,
                    },
                });

            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: swapchain.extent,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&color_attachment)),
            );

            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: swapchain.extent.width as f32,
                    height: swapchain.extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: swapchain.extent,
                }],
            );

            device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.mesh_pipeline.handle,
            );

            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.mesh_pipeline.layout,
                0,
                &[assets.bindless.set, self.material_buffer.set],
                &[],
            );

            for (i, dc) in draw_calls.iter().enumerate() {
                let model = dc.transform.matrix();
                let mvp = view_proj * model;

                let pc = MeshPushConstants {
                    mvp: mvp.to_cols_array_2d(),
                    model: model.to_cols_array_2d(),
                    material_id: i as u32,
                };

                let pc_bytes = std::slice::from_raw_parts(
                    &pc as *const MeshPushConstants as *const u8,
                    std::mem::size_of::<MeshPushConstants>(),
                );

                device.cmd_push_constants(
                    cmd,
                    self.mesh_pipeline.layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    pc_bytes,
                );

                device.cmd_bind_vertex_buffers(cmd, 0, &[dc.gpu_mesh.vertex_buffer], &[0]);
                device.cmd_bind_index_buffer(
                    cmd,
                    dc.gpu_mesh.index_buffer,
                    0,
                    vk::IndexType::UINT32,
                );
                device.cmd_draw_indexed(cmd, dc.gpu_mesh.index_count, 1, 0, 0, 0);
            }

            device.cmd_end_rendering(cmd);

            Self::transition_image(
                device,
                cmd,
                swapchain.images[image_index as usize],
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::PRESENT_SRC_KHR,
            );

            device.end_command_buffer(cmd)?;
        }

        let wait_semaphores = [frame.image_available];
        let signal_semaphores = [frame.render_finished];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let cmds = [cmd];

        unsafe {
            device.queue_submit(
                ctx.device.graphics_queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&cmds)
                    .signal_semaphores(&signal_semaphores)],
                frame.render_fence,
            )?;

            self.swapchain_loader.queue_present(
                ctx.device.present_queue,
                &vk::PresentInfoKHR::default()
                    .wait_semaphores(&signal_semaphores)
                    .swapchains(&[swapchain.handle])
                    .image_indices(&[image_index]),
            )?;
        }

        self.current_frame = (self.current_frame + 1) % FRAMES_IN_FLIGHT as usize;
        Ok(())
    }

    fn transition_image(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        from: vk::ImageLayout,
        to: vk::ImageLayout,
    ) {
        let (src_stage, src_access, dst_stage, dst_access) = match (from, to) {
            (vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) => (
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                vk::AccessFlags2::empty(),
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            ),
            (vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::ImageLayout::PRESENT_SRC_KHR) => (
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                vk::AccessFlags2::empty(),
            ),
            _ => panic!("transition_image: неизвестная пара layout-ов"),
        };

        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
            .dst_access_mask(dst_access)
            .old_layout(from)
            .new_layout(to)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        unsafe {
            device.cmd_pipeline_barrier2(
                cmd,
                &vk::DependencyInfo::default()
                    .image_memory_barriers(std::slice::from_ref(&barrier)),
            )
        };
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe { self.device.handle.device_wait_idle().ok() };
    }
}
