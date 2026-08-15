use crate::render::resource::desc::ResourceHandle;
use crate::render::resource::make_barrier;
use crate::render::resource::pool::ResourcePool;
use ash::vk;
use std::collections::HashMap;

pub struct LayoutTracker {
    layouts: HashMap<ResourceHandle, vk::ImageLayout>,
    scratch: Vec<vk::ImageMemoryBarrier2<'static>>,
}

impl LayoutTracker {
    pub fn new() -> Self {
        Self { layouts: HashMap::new(), scratch: Vec::new() }
    }

    pub fn current(&self, handle: ResourceHandle) -> vk::ImageLayout {
        self.layouts.get(&handle).copied().unwrap_or(vk::ImageLayout::UNDEFINED)
    }

    pub fn set(&mut self, handle: ResourceHandle, layout: vk::ImageLayout) {
        self.layouts.insert(handle, layout);
    }

    /// Computes the list of required barriers for transitioning to the new layouts
    /// and records the new state in the tracker. Does not record GPU commands itself -
    /// the caller must pass the returned slice to `cmd_pipeline_barrier2`
    /// (see `RenderGraph::execute`), or ignore it if it decides to skip the transition.
    pub fn plan_transition(
        &mut self,
        pool: &ResourcePool,
        transitions: impl IntoIterator<Item = (ResourceHandle, vk::ImageLayout)>,
    ) -> &[vk::ImageMemoryBarrier2<'static>] {
        self.scratch.clear();
        for (handle, new_layout) in transitions {
            let old_layout = self.current(handle);
            if old_layout == new_layout {
                continue;
            }
            let img = pool.image(handle);
            self.scratch.push(make_barrier(img.image, img.kind, old_layout, new_layout));
            self.layouts.insert(handle, new_layout);
        }
        &self.scratch
    }

    pub fn invalidate(&mut self, handles: &[ResourceHandle]) {
        for h in handles {
            self.layouts.insert(*h, vk::ImageLayout::UNDEFINED);
        }
    }
}

impl Default for LayoutTracker {
    fn default() -> Self {
        Self::new()
    }
}
