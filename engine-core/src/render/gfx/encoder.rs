use super::handles::{PipelineId, ShaderStage};
use super::pipeline_cache::PipelineCache;
use crate::assets::mesh::GpuMesh;
use crate::render::resource::{ResourceHandle, ResourcePool};
use crate::vulkan::core::debug::{cmd_begin_label, cmd_end_label};
use crate::vulkan::gfx_pipeline::builder::cmd::{
    begin_rendering_clear, begin_rendering_depth_only, begin_rendering_discard, begin_rendering_load,
};
use ash::vk;

pub struct CommandEncoder<'a> {
    device: &'a ash::Device,
    cmd: vk::CommandBuffer,
    pool: &'a ResourcePool,
    pipelines: &'a PipelineCache,
    bound_pipeline: Option<PipelineId>,
}

impl<'a> CommandEncoder<'a> {
    pub(crate) fn new(
        device: &'a ash::Device,
        cmd: vk::CommandBuffer,
        pool: &'a ResourcePool,
        pipelines: &'a PipelineCache,
    ) -> Self {
        Self { device, cmd, pool, pipelines, bound_pipeline: None }
    }

    pub fn raw_cmd(&self) -> vk::CommandBuffer {
        self.cmd
    }

    fn image(&self, handle: ResourceHandle) -> crate::render::resource::ImageRef<'_> {
        self.pool.image(handle)
    }

    pub fn begin_rendering_depth_only(&self, depth: ResourceHandle) {
        let img = self.image(depth);
        begin_rendering_depth_only(self.device, self.cmd, img.view, img.extent);
    }

    pub fn begin_rendering_discard(&self, color: ResourceHandle) {
        let img = self.image(color);
        begin_rendering_discard(self.device, self.cmd, img.view, img.extent);
    }

    pub fn begin_rendering_load(&self, color: ResourceHandle) {
        let img = self.image(color);
        begin_rendering_load(self.device, self.cmd, img.view, img.extent);
    }

    pub fn begin_rendering_clear(&self, color: ResourceHandle, clear: [f32; 4]) {
        let img = self.image(color);
        begin_rendering_clear(self.device, self.cmd, img.view, img.extent, clear);
    }

    pub fn begin_rendering_gbuffer(
        &self,
        albedo: ResourceHandle,
        normal: ResourceHandle,
        depth: ResourceHandle,
        clear_color: [f32; 4],
    ) {
        let albedo_img = self.image(albedo);
        let normal_img = self.image(normal);
        let depth_img = self.image(depth);
        crate::vulkan::gfx_pipeline::builder::cmd::begin_rendering_with_depth(
            self.device,
            self.cmd,
            &[(albedo_img.view, clear_color), (normal_img.view, [0.0; 4])],
            depth_img.view,
            albedo_img.extent,
        );
    }

    pub fn blit_to_swapchain(&self, src: ResourceHandle, dst: ResourceHandle) {
        let src_img = self.image(src);
        let dst_img = self.image(dst);
        let blit = vk::ImageBlit2::default()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D { x: src_img.extent.width as i32, y: src_img.extent.height as i32, z: 1 },
            ])
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D { x: dst_img.extent.width as i32, y: dst_img.extent.height as i32, z: 1 },
            ]);

        unsafe {
            self.device.cmd_blit_image2(
                self.cmd,
                &vk::BlitImageInfo2::default()
                    .src_image(src_img.image)
                    .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .dst_image(dst_img.image)
                    .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .regions(std::slice::from_ref(&blit))
                    .filter(vk::Filter::LINEAR),
            );
        }
    }

    pub fn end_rendering(&self) {
        unsafe { self.device.cmd_end_rendering(self.cmd) };
    }

    pub fn bind_pipeline(&mut self, pipeline: PipelineId) {
        let stored = self.pipelines.get(pipeline);
        unsafe { self.device.cmd_bind_pipeline(self.cmd, vk::PipelineBindPoint::GRAPHICS, stored.handle) };
        self.bound_pipeline = Some(pipeline);
    }

    pub fn bind_descriptor_sets(&self, pipeline: PipelineId, sets: &[vk::DescriptorSet]) {
        let stored = self.pipelines.get(pipeline);
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                self.cmd,
                vk::PipelineBindPoint::GRAPHICS,
                stored.layout,
                0,
                sets,
                &[],
            );
        }
    }

    pub fn push_constants<T: bytemuck::Pod>(&self, pipeline: PipelineId, stage: ShaderStage, data: &T) {
        let stored = self.pipelines.get(pipeline);
        let bytes = bytemuck::bytes_of(data);
        unsafe { self.device.cmd_push_constants(self.cmd, stored.layout, stage.to_vk(), 0, bytes) };
    }

    pub fn bind_mesh(&self, mesh: &GpuMesh) {
        unsafe {
            self.device.cmd_bind_vertex_buffers(self.cmd, 0, &[mesh.vertex_buffer], &[0]);
            self.device.cmd_bind_index_buffer(self.cmd, mesh.index_buffer, 0, vk::IndexType::UINT32);
        }
    }

    pub fn draw_indexed(&self, index_count: u32) {
        unsafe { self.device.cmd_draw_indexed(self.cmd, index_count, 1, 0, 0, 0) };
    }

    pub fn draw(&self, vertex_count: u32) {
        unsafe { self.device.cmd_draw(self.cmd, vertex_count, 1, 0, 0) };
    }

    pub fn set_debug_label(&self, debug_utils: Option<&ash::ext::debug_utils::Device>, name: &str) {
        if let Some(du) = debug_utils {
            cmd_begin_label(du, self.cmd, name);
        }
    }

    pub fn end_debug_label(&self, debug_utils: Option<&ash::ext::debug_utils::Device>) {
        if let Some(du) = debug_utils {
            cmd_end_label(du, self.cmd);
        }
    }

    pub fn extent_of(&self, handle: ResourceHandle) -> [f32; 2] {
        let img = self.image(handle);
        [img.extent.width as f32, img.extent.height as f32]
    }
}
