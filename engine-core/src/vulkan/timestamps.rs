use ash::vk;

#[derive(Debug, Clone, Default)]
pub struct GpuFrameTimes {
    pub passes: Vec<(String, f32)>,
    pub total_ms: f32,
}

struct FramePool {
    pool: vk::QueryPool,
    submitted_count: u32,
}

pub struct GpuTimestampPool {
    pools: Vec<FramePool>,
    pass_names: Vec<String>,
    timestamp_period: f32,
    query_count: u32,
    device: ash::Device,
    pub last_frame: GpuFrameTimes,
}

impl GpuTimestampPool {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        frames_in_flight: u32,
        pass_names: Vec<String>,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        let timestamp_period = props.limits.timestamp_period;
        if timestamp_period == 0.0 {
            anyhow::bail!("GPU does not support timestamp queries");
        }

        let query_count = (pass_names.len() * 2) as u32;

        let mut raw_pools = Vec::with_capacity(frames_in_flight as usize);
        for _ in 0..frames_in_flight {
            let pool = unsafe {
                device.create_query_pool(
                    &vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(query_count),
                    None,
                )?
            };
            raw_pools.push(pool);
        }

        initial_reset_all(device, command_pool, queue, &raw_pools, query_count)?;

        let pools = raw_pools.into_iter().map(|pool| FramePool { pool, submitted_count: 0 }).collect();

        log::debug!("GpuTimestampPool: {} frames x {} passes x 2 queries", frames_in_flight, pass_names.len());

        Ok(Self {
            pools,
            pass_names,
            timestamp_period,
            query_count,
            device: device.clone(),
            last_frame: GpuFrameTimes::default(),
        })
    }

    pub fn read_and_reset(&mut self, frame_index: usize, cmd: vk::CommandBuffer) {
        let fp = &mut self.pools[frame_index];

        if fp.submitted_count > 0 {
            let mut raw = vec![0u64; self.query_count as usize];
            match unsafe { self.device.get_query_pool_results(fp.pool, 0, &mut raw, vk::QueryResultFlags::TYPE_64) } {
                Ok(()) => {
                    let period_ms = self.timestamp_period * 1e-6;
                    let mut passes = Vec::with_capacity(self.pass_names.len());
                    let mut total = 0.0f32;

                    for (i, name) in self.pass_names.iter().enumerate() {
                        let begin = raw[i * 2];
                        let end = raw[i * 2 + 1];
                        let ms = if end > begin {
                            (end - begin) as f32 * period_ms
                        } else {
                            0.0
                        };
                        passes.push((name.clone(), ms));
                        total += ms;
                    }

                    self.last_frame = GpuFrameTimes { passes, total_ms: total };
                }
                Err(vk::Result::NOT_READY) => {}
                Err(e) => log::warn!("GpuTimestampPool read failed: {:?}", e),
            }
        }

        unsafe {
            self.device.cmd_reset_query_pool(cmd, fp.pool, 0, self.query_count);
        }
    }

    pub fn begin_pass(&self, cmd: vk::CommandBuffer, frame_index: usize, pass_index: usize) {
        unsafe {
            self.device.cmd_write_timestamp2(
                cmd,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                self.pools[frame_index].pool,
                (pass_index * 2) as u32,
            );
        }
    }

    pub fn end_pass(&self, cmd: vk::CommandBuffer, frame_index: usize, pass_index: usize) {
        unsafe {
            self.device.cmd_write_timestamp2(
                cmd,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                self.pools[frame_index].pool,
                (pass_index * 2 + 1) as u32,
            );
        }
    }

    pub fn mark_submitted(&mut self, frame_index: usize) {
        self.pools[frame_index].submitted_count += 1;
    }
}

impl Drop for GpuTimestampPool {
    fn drop(&mut self) {
        for fp in &self.pools {
            unsafe { self.device.destroy_query_pool(fp.pool, None) };
        }
    }
}

fn initial_reset_all(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    pools: &[vk::QueryPool],
    query_count: u32,
) -> anyhow::Result<()> {
    let cmd = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0]
    };
    unsafe {
        device.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        for &pool in pools {
            device.cmd_reset_query_pool(cmd, pool, 0, query_count);
        }
        device.end_command_buffer(cmd)?;
        device.queue_submit(
            queue,
            &[vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd))],
            vk::Fence::null(),
        )?;
        device.queue_wait_idle(queue)?;
        device.free_command_buffers(command_pool, &[cmd]);
    }
    Ok(())
}
