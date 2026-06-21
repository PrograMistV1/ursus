use crate::render::resource::{
    make_barrier, DescriptorBinding, DescriptorBindingRegistry, DescriptorImageType, LayoutTracker, ResourceHandle,
    ResourcePool,
};
use crate::vulkan::core::debug::{cmd_begin_label, cmd_end_label};
use crate::vulkan::timestamps::{GpuFrameTimes, GpuTimestampPool};
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

struct FrameDataBox(*mut ());
unsafe impl Send for FrameDataBox {}

impl PassAccess {
    pub fn read(handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        Self { handle, access: AccessType::Read, layout }
    }

    pub fn write(handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        Self { handle, access: AccessType::Write, layout }
    }

    pub fn read_write(handle: ResourceHandle, layout: vk::ImageLayout) -> Self {
        Self { handle, access: AccessType::ReadWrite, layout }
    }
}

pub type RecordFn = Box<dyn FnMut(vk::CommandBuffer, &ResourcePool, *mut ()) -> anyhow::Result<()> + Send>;

pub struct PassNode {
    pub name: String,
    pub accesses: Vec<PassAccess>,
    pub record: RecordFn,
    pub enabled: bool,
    pub depends_on: Vec<PassHandle>,
}

impl PassNode {
    pub fn new(name: impl Into<String>, accesses: Vec<PassAccess>, record: RecordFn) -> Self {
        Self { name: name.into(), accesses, record, enabled: true, depends_on: Vec::new() }
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

#[derive(Clone)]
struct CompiledBarrier {
    handle: ResourceHandle,
    new_layout: vk::ImageLayout,
}

struct CompiledPass {
    node_index: usize,
    barriers: Vec<CompiledBarrier>,
}

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
    allocated: bool,

    compiled_passes: Vec<CompiledPass>,
    compiled_finals: Vec<CompiledBarrier>,

    frame_data: Option<FrameDataBox>,
    frame_data_drop: Option<Box<dyn FnOnce(*mut ()) + Send>>,

    barrier_scratch: Vec<vk::ImageMemoryBarrier2<'static>>,

    timestamps: Option<GpuTimestampPool>,
    pub last_frame_times: Option<GpuFrameTimes>,
    current_frame: usize,
    frames_in_flight: usize,
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
            allocated: false,
            compiled_passes: Vec::new(),
            compiled_finals: Vec::new(),
            debug_utils,
            frame_data: None,
            frame_data_drop: None,
            barrier_scratch: Vec::new(),
            timestamps: None,
            last_frame_times: None,
            current_frame: 0,
            frames_in_flight: 0,
        }
    }

    pub fn enable_timestamps(
        &mut self,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        frames_in_flight: u32,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<()> {
        assert!(self.compiled, "enable_timestamps вызван до compile()");

        let pass_names = self.nodes.iter().map(|n| n.name.clone()).collect();

        self.timestamps = Some(GpuTimestampPool::new(
            device,
            physical_device,
            instance,
            frames_in_flight,
            pass_names,
            command_pool,
            queue,
        )?);
        self.frames_in_flight = frames_in_flight as usize;
        Ok(())
    }

    pub fn disable_timestamps(&mut self) {
        self.timestamps = None;
        self.last_frame_times = None;
    }

    pub fn add_pass(&mut self, node: PassNode) -> PassHandle {
        for access in &node.accesses {
            let extra = match access.layout {
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => vk::ImageUsageFlags::SAMPLED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => vk::ImageUsageFlags::COLOR_ATTACHMENT,
                vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL => vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
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

        build_resource_edges(&self.nodes, &mut adj, &mut in_degree);
        build_explicit_edges(&self.nodes, &mut adj, &mut in_degree);

        self.sorted_order = topological_sort(n, adj, in_degree)?;
        self.compiled = true;

        self.build_compiled_passes();

        log::info!(
            "RenderGraph скомпилирован: {} пассов -> {:?}",
            n,
            self.nodes.iter().enumerate().map(|(i, p)| format!("[{}]{}", i, p.name)).collect::<Vec<_>>()
        );
        Ok(())
    }

    fn build_compiled_passes(&mut self) {
        let mut sim_layouts: HashMap<ResourceHandle, vk::ImageLayout> = HashMap::new();

        self.compiled_passes.clear();
        self.compiled_finals.clear();

        for &idx in &self.sorted_order {
            let node = &self.nodes[idx];
            if !node.enabled {
                self.compiled_passes.push(CompiledPass { node_index: idx, barriers: Vec::new() });
                continue;
            }

            let mut barriers = Vec::new();
            for access in &node.accesses {
                let old = sim_layouts.get(&access.handle).copied().unwrap_or(vk::ImageLayout::UNDEFINED);
                if old != access.layout {
                    barriers.push(CompiledBarrier { handle: access.handle, new_layout: access.layout });
                    sim_layouts.insert(access.handle, access.layout);
                }
            }

            self.compiled_passes.push(CompiledPass { node_index: idx, barriers });
        }

        for handle in self.pool.external_handles().collect::<Vec<_>>() {
            if let Some(final_layout) = self.pool.external_final_layout(handle) {
                self.compiled_finals.push(CompiledBarrier { handle, new_layout: final_layout });
            }
        }
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
        self.pool.allocate(self.internal_resolution, self.output_resolution)?;
        self.bindings.flush_all(&self.pool);
        self.allocated = true;
        Ok(())
    }

    pub fn reset_external_layouts(&mut self) {
        for handle in self.pool.external_handles().collect::<Vec<_>>() {
            if let Some(initial) = self.pool.external_initial_layout(handle) {
                self.tracker.set(handle, initial);
            }
        }
    }

    pub fn execute(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) -> anyhow::Result<()> {
        assert!(self.compiled, "RenderGraph::compile() не был вызван");

        if let Some(ts) = &mut self.timestamps {
            ts.read_and_reset(self.current_frame, cmd);
        }

        let frame_ptr = self.frame_data.as_ref().map(|b| b.0).unwrap_or(std::ptr::null_mut());

        for (order_idx, cp) in self.compiled_passes.iter().enumerate() {
            let node = &mut self.nodes[cp.node_index];
            if !node.enabled {
                continue;
            }

            if let Some(du) = &self.debug_utils {
                cmd_begin_label(du, cmd, &node.name);
            }

            if !cp.barriers.is_empty() {
                self.barrier_scratch.clear();
                for cb in &cp.barriers {
                    let old = self.tracker.current(cb.handle);
                    if old == cb.new_layout {
                        continue;
                    }
                    let img = self.pool.image(cb.handle);
                    self.barrier_scratch.push(make_barrier(img.image, img.kind, old, cb.new_layout));
                    self.tracker.set(cb.handle, cb.new_layout);
                }
                if !self.barrier_scratch.is_empty() {
                    unsafe {
                        device.cmd_pipeline_barrier2(
                            cmd,
                            &vk::DependencyInfo::default().image_memory_barriers(&self.barrier_scratch),
                        );
                    }
                }
            }

            if let Some(ts) = &self.timestamps {
                ts.begin_pass(cmd, self.current_frame, order_idx);
            }

            (node.record)(cmd, &self.pool, frame_ptr)?;

            if let Some(ts) = &self.timestamps {
                ts.end_pass(cmd, self.current_frame, order_idx);
            }

            if let Some(du) = &self.debug_utils {
                cmd_end_label(du, cmd);
            }
        }

        if !self.compiled_finals.is_empty() {
            self.barrier_scratch.clear();
            for cb in &self.compiled_finals {
                let old = self.tracker.current(cb.handle);
                if old == cb.new_layout {
                    continue;
                }
                let img = self.pool.image(cb.handle);
                self.barrier_scratch.push(make_barrier(img.image, img.kind, old, cb.new_layout));
                self.tracker.set(cb.handle, cb.new_layout);
            }
            if !self.barrier_scratch.is_empty() {
                unsafe {
                    device.cmd_pipeline_barrier2(
                        cmd,
                        &vk::DependencyInfo::default().image_memory_barriers(&self.barrier_scratch),
                    );
                }
            }
        }

        if let Some(ts) = &self.timestamps {
            self.last_frame_times = Some(ts.last_frame.clone());
        }

        self.current_frame = (self.current_frame + 1) % self.frames_in_flight.max(1);
        Ok(())
    }

    pub fn mark_submitted(&mut self) {
        if let Some(ts) = &mut self.timestamps {
            ts.mark_submitted(self.current_frame);
        }
    }

    pub fn resize_output(&mut self, new_output: (u32, u32)) -> anyhow::Result<()> {
        self.output_resolution = new_output;
        self.pool.resize_output(self.internal_resolution, new_output)?;
        let affected: Vec<ResourceHandle> = self.pool.output_handles().collect();
        self.bindings.flush(&self.pool, &affected);
        self.tracker.invalidate(&affected);
        self.build_compiled_passes();
        Ok(())
    }

    pub fn resize_internal(&mut self, new_internal: (u32, u32)) -> anyhow::Result<()> {
        self.internal_resolution = new_internal;
        self.pool.resize_internal(new_internal, self.output_resolution)?;
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

    pub fn set_frame_data<T: Send + 'static>(&mut self, data: Box<T>) {
        self.drop_frame_data_inner();

        let ptr = Box::into_raw(data) as *mut ();
        self.frame_data = Some(FrameDataBox(ptr));
        self.frame_data_drop = Some(Box::new(|p| unsafe {
            drop(Box::from_raw(p as *mut T));
        }));
    }

    pub fn frame_data_ptr(&self) -> *mut () {
        self.frame_data.as_ref().map(|b| b.0).unwrap_or(std::ptr::null_mut())
    }

    fn drop_frame_data_inner(&mut self) {
        if let Some(drop_fn) = self.frame_data_drop.take() {
            if let Some(FrameDataBox(ptr)) = self.frame_data.take() {
                drop_fn(ptr);
            }
        }
    }
}

impl Drop for RenderGraph {
    fn drop(&mut self) {
        self.drop_frame_data_inner();
    }
}

fn build_resource_edges(nodes: &[PassNode], adj: &mut Vec<HashSet<usize>>, in_degree: &mut Vec<usize>) {
    let mut last_writer: HashMap<ResourceHandle, usize> = HashMap::new();
    let mut last_readers: HashMap<ResourceHandle, Vec<usize>> = HashMap::new();

    for (i, node) in nodes.iter().enumerate() {
        for access in &node.accesses {
            match access.access {
                AccessType::Read => {
                    add_edge(adj, in_degree, last_writer.get(&access.handle).copied(), i);
                    last_readers.entry(access.handle).or_default().push(i);
                }
                AccessType::Write | AccessType::ReadWrite => {
                    add_edge(adj, in_degree, last_writer.get(&access.handle).copied(), i);
                    add_edges_from_readers(adj, in_degree, &last_readers, access.handle, i);
                    last_writer.insert(access.handle, i);
                    last_readers.remove(&access.handle);
                }
            }
        }
    }
}

fn build_explicit_edges(nodes: &[PassNode], adj: &mut Vec<HashSet<usize>>, in_degree: &mut Vec<usize>) {
    for (i, node) in nodes.iter().enumerate() {
        for &dep_handle in &node.depends_on {
            add_edge(adj, in_degree, Some(dep_handle.0 as usize), i);
        }
    }
}

fn add_edge(adj: &mut Vec<HashSet<usize>>, in_degree: &mut Vec<usize>, from: Option<usize>, to: usize) {
    let Some(from) = from else { return };
    if from != to && !adj[from].contains(&to) {
        adj[from].insert(to);
        in_degree[to] += 1;
    }
}

fn add_edges_from_readers(
    adj: &mut Vec<HashSet<usize>>,
    in_degree: &mut Vec<usize>,
    last_readers: &HashMap<ResourceHandle, Vec<usize>>,
    handle: ResourceHandle,
    to: usize,
) {
    let Some(readers) = last_readers.get(&handle) else {
        return;
    };
    for &reader in readers {
        add_edge(adj, in_degree, Some(reader), to);
    }
}

fn topological_sort(n: usize, adj: Vec<HashSet<usize>>, mut in_degree: Vec<usize>) -> anyhow::Result<Vec<usize>> {
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
    Ok(order)
}

pub struct PassBuilder {
    name: String,
    accesses: Vec<PassAccess>,
    deferred_bindings: Vec<DescriptorBinding>,
    explicit_deps: Vec<PassHandle>,
}

impl PassBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), accesses: Vec::new(), deferred_bindings: Vec::new(), explicit_deps: Vec::new() }
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

    pub fn after(mut self, handle: PassHandle) -> Self {
        self.explicit_deps.push(handle);
        self
    }

    pub fn record<F>(self, f: F) -> PassNodeReady
    where
        F: FnMut(vk::CommandBuffer, &ResourcePool, *mut ()) -> anyhow::Result<()> + Send + 'static,
    {
        PassNodeReady {
            node: PassNode { depends_on: self.explicit_deps, ..PassNode::new(self.name, self.accesses, Box::new(f)) },
            deferred_bindings: self.deferred_bindings,
        }
    }
}

pub struct PassNodeReady {
    node: PassNode,
    deferred_bindings: Vec<DescriptorBinding>,
}

impl PassNodeReady {
    pub fn build(self, graph: &mut RenderGraph) -> PassHandle {
        for b in self.deferred_bindings {
            let resource = b.resource;
            graph.bindings.register(b);
            if graph.allocated {
                graph.bindings.flush(&graph.pool, &[resource]);
            }
        }
        graph.add_pass(self.node)
    }
}

pub fn pass(name: impl Into<String>) -> PassBuilder {
    PassBuilder::new(name)
}
