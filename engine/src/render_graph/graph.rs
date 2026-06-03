use crate::render_graph::resource::{
    DescriptorBinding, DescriptorBindingRegistry, DescriptorImageType, LayoutTracker,
    ResourceHandle, ResourcePool,
};
use crate::vulkan::core::debug::{cmd_begin_label, cmd_end_label};
use ash::ext::debug_utils;
use ash::vk;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessType {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone)]
pub struct PassAccess {
    pub handle: ResourceHandle,
    pub access: AccessType,
    pub layout: vk::ImageLayout,
}

impl PassAccess {
    pub fn read(handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        Self {
            handle,
            access: AccessType::Read,
            layout,
        }
    }

    pub fn write(handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        Self {
            handle,
            access: AccessType::Write,
            layout,
        }
    }

    pub fn read_write(handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        Self {
            handle,
            access: AccessType::ReadWrite,
            layout,
        }
    }
}

pub type RecordFn =
    Box<dyn FnMut(vk::CommandBuffer, &ResourcePool, *mut ()) -> anyhow::Result<()> + Send>;

pub struct PassNode {
    pub name: String,
    pub accesses: Vec<PassAccess>,
    pub record: RecordFn,
    pub enabled: bool,
    pub depends_on: Vec<PassHandle>,
}

impl PassNode {
    pub fn new(name: impl Into<String>, accesses: Vec<PassAccess>, record: RecordFn) -> Self {
        Self {
            name: name.into(),
            accesses,
            record,
            enabled: true,
            depends_on: Vec::new(),
        }
    }
}

impl std::fmt::Debug for PassNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PassNode")
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("accesses", &self.accesses.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassHandle(pub(crate) u32);

pub struct RenderGraph {
    pub pool: ResourcePool,
    nodes: Vec<PassNode>,
    sorted_order: Vec<usize>,
    tracker: LayoutTracker,
    bindings: DescriptorBindingRegistry,
    debug_utils: Option<Arc<debug_utils::Device>>,

    internal_resolution: (u32, u32),
    output_resolution: (u32, u32),

    compiled: bool,
}

impl RenderGraph {
    pub fn new(
        pool: ResourcePool,
        device: ash::Device,
        internal_resolution: (u32, u32),
        output_resolution: (u32, u32),
        debug_utils: Option<Arc<debug_utils::Device>>,
    ) -> Self {
        Self {
            bindings: DescriptorBindingRegistry::new(device),
            pool,
            nodes: Vec::new(),
            sorted_order: Vec::new(),
            tracker: LayoutTracker::new(),
            internal_resolution,
            output_resolution,
            compiled: false,
            debug_utils,
        }
    }

    pub fn add_pass(&mut self, node: PassNode) -> PassHandle {
        for access in &node.accesses {
            let extra = match access.layout {
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => vk::ImageUsageFlags::SAMPLED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => vk::ImageUsageFlags::COLOR_ATTACHMENT,
                vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL => {
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                }
                _ => vk::ImageUsageFlags::empty(),
            };
            self.pool.add_usage(access.handle, extra);
        }

        let handle = PassHandle(self.nodes.len() as u32);
        self.nodes.push(node);
        self.compiled = false;
        handle
    }

    pub fn compile(&mut self) -> anyhow::Result<()> {
        let n = self.nodes.len();
        let mut adj: Vec<HashSet<usize>> = vec![HashSet::new(); n];
        let mut in_degree = vec![0usize; n];

        let mut last_writer: HashMap<ResourceHandle, usize> = HashMap::new();

        for (i, node) in self.nodes.iter().enumerate() {
            for access in &node.accesses {
                if matches!(access.access, AccessType::Read) {
                    if let Some(&writer) = last_writer.get(&access.handle) {
                        if writer != i && !adj[writer].contains(&i) {
                            adj[writer].insert(i);
                            in_degree[i] += 1;
                        }
                    }
                }
                if matches!(access.access, AccessType::Write | AccessType::ReadWrite) {
                    last_writer.insert(access.handle, i);
                }
            }
        }

        for (i, node) in self.nodes.iter().enumerate() {
            for &dep_handle in &node.depends_on {
                let dep = dep_handle.0 as usize;
                if dep != i && !adj[dep].contains(&i) {
                    adj[dep].insert(i);
                    in_degree[i] += 1;
                }
            }
        }

        let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);

        while let Some(idx) = queue.pop_front() {
            order.push(idx);
            for &dep in &adj[idx] {
                in_degree[dep] -= 1;
                if in_degree[dep] == 0 {
                    queue.push_back(dep);
                }
            }
        }

        if order.len() != n {
            anyhow::bail!("RenderGraph: обнаружен цикл в графе пассов");
        }

        self.sorted_order = order;
        self.compiled = true;

        log::info!(
            "RenderGraph скомпилирован: {} пассов → {:?}",
            n,
            self.nodes
                .iter()
                .enumerate()
                .map(|(i, p)| format!("[{}]{}", i, p.name))
                .collect::<Vec<_>>()
        );

        Ok(())
    }

    pub fn bind_resource(&mut self, binding: DescriptorBinding) {
        self.bindings.register(binding);
    }

    pub fn bind_sampled(
        &mut self,
        resource: ResourceHandle,
        set: vk::DescriptorSet,
        binding: u32,
        sampler: vk::Sampler,
    ) {
        self.bindings.register(DescriptorBinding {
            resource,
            set,
            binding,
            array_element: 0,
            image_type: DescriptorImageType::CombinedImageSampler(sampler),
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        });
    }

    pub fn allocate(&mut self) -> anyhow::Result<()> {
        self.pool
            .allocate(self.internal_resolution, self.output_resolution)?;
        self.bindings.flush_all(&self.pool);
        Ok(())
    }

    pub fn execute(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        ctx: *mut (),
    ) -> anyhow::Result<()> {
        assert!(self.compiled, "RenderGraph::compile() не был вызван");

        for &idx in &self.sorted_order {
            let node = &self.nodes[idx];
            if !node.enabled {
                continue;
            }

            if let Some(du) = &self.debug_utils {
                cmd_begin_label(du, cmd, &node.name);
            }

            let transitions: Vec<(ResourceHandle, vk::ImageLayout)> =
                node.accesses.iter().map(|a| (a.handle, a.layout)).collect();

            self.tracker
                .transition(device, cmd, &self.pool, &transitions);

            let node = &mut self.nodes[idx];
            (node.record)(cmd, &self.pool, ctx)?;

            if let Some(du) = &self.debug_utils {
                cmd_end_label(du, cmd);
            }
        }

        Ok(())
    }

    pub fn resize_output(&mut self, new_output: (u32, u32)) -> anyhow::Result<()> {
        self.output_resolution = new_output;
        self.pool
            .resize_output(self.internal_resolution, new_output)?;

        let affected: Vec<ResourceHandle> = self.pool.output_handles().collect();
        self.bindings.flush(&self.pool, &affected);
        self.tracker.invalidate(&affected);
        Ok(())
    }

    pub fn resize_internal(&mut self, new_internal: (u32, u32)) -> anyhow::Result<()> {
        self.internal_resolution = new_internal;
        self.pool
            .resize_internal(new_internal, self.output_resolution)?;

        let affected: Vec<ResourceHandle> = self.pool.internal_handles().collect();
        self.bindings.flush(&self.pool, &affected);
        self.tracker.invalidate(&affected);
        Ok(())
    }

    pub fn internal_resolution(&self) -> (u32, u32) {
        self.internal_resolution
    }

    pub fn output_resolution(&self) -> (u32, u32) {
        self.output_resolution
    }

    pub fn pass_mut(&mut self, handle: PassHandle) -> &mut PassNode {
        &mut self.nodes[handle.0 as usize]
    }
}

pub struct PassBuilder {
    name: String,
    accesses: Vec<PassAccess>,
    deferred_bindings: Vec<DescriptorBinding>,
    explicit_deps: Vec<PassHandle>,
}

impl PassBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            accesses: Vec::new(),
            deferred_bindings: Vec::new(),
            explicit_deps: Vec::new(),
        }
    }

    pub fn read(mut self, handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        self.accesses.push(PassAccess::read(handle, layout));
        self
    }

    pub fn write(mut self, handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        self.accesses.push(PassAccess::write(handle, layout));
        self
    }

    pub fn read_write(mut self, handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        self.accesses.push(PassAccess::read_write(handle, layout));
        self
    }

    pub fn bind_resource(mut self, binding: DescriptorBinding) -> Self {
        self.deferred_bindings.push(binding);
        self
    }

    pub fn bind_sampled(
        mut self,
        resource: ResourceHandle,
        set: vk::DescriptorSet,
        binding: u32,
        sampler: vk::Sampler,
    ) -> Self {
        self.deferred_bindings.push(DescriptorBinding {
            resource,
            set,
            binding,
            array_element: 0,
            image_type: DescriptorImageType::CombinedImageSampler(sampler),
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        });
        self
    }

    pub fn bind_sampled_at(
        mut self,
        resource: ResourceHandle,
        set: vk::DescriptorSet,
        binding: u32,
        array_element: u32,
        sampler: vk::Sampler,
        image_layout: vk::ImageLayout,
    ) -> Self {
        self.deferred_bindings.push(DescriptorBinding {
            resource,
            set,
            binding,
            array_element,
            image_type: DescriptorImageType::CombinedImageSampler(sampler),
            image_layout,
        });
        self
    }

    pub fn record<F>(self, f: F) -> PassNodeReady
    where
        F: FnMut(vk::CommandBuffer, &ResourcePool, *mut ()) -> anyhow::Result<()> + Send + 'static,
    {
        PassNodeReady {
            node: PassNode {
                depends_on: self.explicit_deps,
                ..PassNode::new(self.name, self.accesses, Box::new(f))
            },
            deferred_bindings: self.deferred_bindings,
        }
    }

    pub fn after(mut self, handle: PassHandle) -> Self {
        self.explicit_deps.push(handle);
        self
    }
}

pub struct PassNodeReady {
    node: PassNode,
    deferred_bindings: Vec<DescriptorBinding>,
}

impl PassNodeReady {
    pub fn build(self, graph: &mut RenderGraph) -> PassHandle {
        for b in self.deferred_bindings {
            graph.bindings.register(b);
        }
        graph.add_pass(self.node)
    }
}

pub fn pass(name: impl Into<String>) -> PassBuilder {
    PassBuilder::new(name)
}
