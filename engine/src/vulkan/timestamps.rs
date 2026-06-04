use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuStage {
    Shadow = 0,
    Geometry = 1,
    Lighting = 2,
    PostProcess = 3,
    FsrEasu = 4,
    FsrRcas = 5,
    Ui = 6,
}

impl GpuStage {
    pub const ALL: &'static [GpuStage] = &[
        GpuStage::Shadow,
        GpuStage::Geometry,
        GpuStage::Lighting,
        GpuStage::PostProcess,
        GpuStage::FsrEasu,
        GpuStage::FsrRcas,
        GpuStage::Ui,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GpuStage::Shadow => "Shadow",
            GpuStage::Geometry => "Geometry",
            GpuStage::Lighting => "Lighting",
            GpuStage::PostProcess => "PostProcess",
            GpuStage::FsrEasu => "FSR EASU",
            GpuStage::FsrRcas => "FSR RCAS",
            GpuStage::Ui => "UI",
        }
    }
}

pub const STAGE_COUNT: usize = 7;
const QUERY_COUNT: u32 = (STAGE_COUNT * 2) as u32;

#[derive(Debug, Clone, Copy, Default)]
pub struct GpuFrameTimes {
    pub pass_ms: [f32; STAGE_COUNT],
    pub total_ms: f32,
}

struct FramePool {
    pool: vk::QueryPool,
    /// Сколько раз этот слот был submitted — читать можно только начиная со второго цикла
    submitted_count: u32,
}

pub struct GpuTimestampPool {
    pools: Vec<FramePool>,
    timestamp_period: f32,
    device: ash::Device,
    pub last_frame: GpuFrameTimes,
}

impl GpuTimestampPool {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        frames_in_flight: u32,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        let timestamp_period = props.limits.timestamp_period;
        if timestamp_period == 0.0 {
            anyhow::bail!("GPU не поддерживает timestamp queries");
        }

        let create_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(QUERY_COUNT);

        let mut raw_pools = Vec::with_capacity(frames_in_flight as usize);
        for _ in 0..frames_in_flight {
            let pool = unsafe { device.create_query_pool(&create_info, None)? };
            raw_pools.push(pool);
        }

        // Начальный сброс: без него первый write_timestamp упадёт с VUID-03864
        initial_reset_all(device, command_pool, queue, &raw_pools)?;

        let pools = raw_pools
            .into_iter()
            .map(|pool| FramePool {
                pool,
                submitted_count: 0,
            })
            .collect();

        log::info!(
            "GpuTimestampPool: {} frames × {} queries, period={:.2}ns",
            frames_in_flight,
            QUERY_COUNT,
            timestamp_period
        );

        Ok(Self {
            pools,
            timestamp_period,
            device: device.clone(),
            last_frame: GpuFrameTimes::default(),
        })
    }

    /// Вызывать в начале draw_frame ПОСЛЕ begin_command_buffer.
    /// Читает результаты предыдущего использования этого слота (если они есть),
    /// затем записывает cmd_reset_query_pool в текущий cmd.
    pub fn read_and_reset(&mut self, frame_index: usize, cmd: vk::CommandBuffer) {
        let fp = &mut self.pools[frame_index];

        // Читаем только если этот слот уже хотя бы раз завершился на GPU.
        // fence уже прошёл => данные гарантированно готовы, NOT_READY не будет.
        if fp.submitted_count > 0 {
            let mut raw = [0u64; QUERY_COUNT as usize];
            match unsafe {
                self.device.get_query_pool_results(
                    fp.pool,
                    0,
                    &mut raw,
                    // Без WAIT: fence уже прошёл, данные должны быть готовы.
                    // Если вдруг NOT_READY — просто оставляем прошлый результат.
                    vk::QueryResultFlags::TYPE_64,
                )
            } {
                Ok(()) => {
                    let period_ms = self.timestamp_period * 1e-6;
                    let mut pass_ms = [0.0f32; STAGE_COUNT];
                    let mut total = 0.0f32;
                    for (i, stage) in GpuStage::ALL.iter().enumerate() {
                        let begin = raw[*stage as usize * 2];
                        let end = raw[*stage as usize * 2 + 1];
                        let ms = if end > begin {
                            (end - begin) as f32 * period_ms
                        } else {
                            0.0
                        };
                        pass_ms[i] = ms;
                        total += ms;
                    }
                    self.last_frame = GpuFrameTimes {
                        pass_ms,
                        total_ms: total,
                    };
                }
                Err(vk::Result::NOT_READY) => { /* оставляем прошлый кадр */ }
                Err(e) => log::warn!("GpuTimestampPool read: {:?}", e),
            }
        }

        // Сбрасываем прямо в command buffer — выполнится на GPU до любого write_timestamp
        unsafe {
            self.device
                .cmd_reset_query_pool(cmd, fp.pool, 0, QUERY_COUNT);
        }
    }

    /// Вызывать после queue_submit для данного frame_index.
    pub fn mark_submitted(&mut self, frame_index: usize) {
        self.pools[frame_index].submitted_count += 1;
    }

    pub fn begin_pass(&self, cmd: vk::CommandBuffer, frame_index: usize, stage: GpuStage) {
        unsafe {
            self.device.cmd_write_timestamp2(
                cmd,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                self.pools[frame_index].pool,
                stage as u32 * 2,
            );
        }
    }

    pub fn end_pass(&self, cmd: vk::CommandBuffer, frame_index: usize, stage: GpuStage) {
        unsafe {
            self.device.cmd_write_timestamp2(
                cmd,
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                self.pools[frame_index].pool,
                stage as u32 * 2 + 1,
            );
        }
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
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        for &pool in pools {
            device.cmd_reset_query_pool(cmd, pool, 0, QUERY_COUNT);
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
