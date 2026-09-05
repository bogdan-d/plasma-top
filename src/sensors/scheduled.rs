use std::collections::{BTreeSet, VecDeque};
use std::time::Duration;

use crate::config::Config;
use crate::domain::boundary::ClockSnapshot;
use crate::domain::metric::Capability;
use crate::domain::readings::{
    DiskUsageReading, DisplaySnapshot, HardwareInventory, MetricSample, RetainedMetricSample,
};
use crate::scheduler::{
    CompletionKind, ConfigGeneration, HistoryDeadline, InventoryGeneration, JobId, JobKind,
    JobTicket, PageId, PeripheralRole, PeripheralSource, RefreshTrigger, Scheduler,
    SchedulerAction, SchedulerEvent, SchedulerTime, SourceIdentity, TimingClass,
};

use super::attempts::{
    AttemptResult, AttemptStatus, Timings, attempt_bolt_peripheral, attempt_cpu, attempt_cpu_cores,
    attempt_disk_io, attempt_disk_temperature, attempt_disk_usage, attempt_external,
    attempt_fan_speed, attempt_memory, attempt_network_info, attempt_network_speed,
    attempt_process, attempt_smart, attempt_system_battery, attempt_upower_peripheral,
};
use super::coordinator::{CollectCtx, OwnerRefs};

use super::process::ProcessPageStatus;
use super::{cpu, memory, network};

#[path = "scheduled/gpu.rs"]
mod gpu;
#[path = "scheduled/power.rs"]
mod power;
use power::execute_peripheral;
#[path = "scheduled/invalidation.rs"]
mod invalidation;
#[path = "scheduled/profiling.rs"]
mod profiling;

pub(crate) use invalidation::invalidate_scheduled_job;
pub(crate) use profiling::capture_time as scheduled_capture_time;

pub(crate) struct JobExecution {
    pub(crate) completion: CompletionKind,
    pub(crate) notification_ready: bool,
    pub(crate) notifications: DisplaySnapshot,
}

pub(crate) trait ScheduledExecutionIdentity {
    fn job(&self) -> &JobId;

    fn history_deadline(&self) -> Option<HistoryDeadline>;

    fn metrics(&self) -> Option<&BTreeSet<crate::domain::Metric>> {
        None
    }
}

impl ScheduledExecutionIdentity for JobId {
    fn job(&self) -> &JobId {
        self
    }

    fn history_deadline(&self) -> Option<HistoryDeadline> {
        None
    }
}

impl ScheduledExecutionIdentity for JobTicket {
    fn metrics(&self) -> Option<&BTreeSet<crate::domain::Metric>> {
        Some(&self.metrics)
    }
    fn job(&self) -> &JobId {
        &self.job
    }

    fn history_deadline(&self) -> Option<HistoryDeadline> {
        self.history_deadline
    }
}

pub(crate) struct SerialSchedule {
    scheduler: Scheduler,
    readings: DisplaySnapshot,
    pending: VecDeque<SchedulerAction>,
    display_jobs: Vec<JobId>,
    sampled_once: bool,
}

impl SerialSchedule {
    pub(crate) fn new(
        cfg: &Config,
        hw: &HardwareInventory,
        proc_root: &std::path::Path,
        at: SchedulerTime,
        selected_page: PageId,
    ) -> Self {
        let auto_mounts = super::disk::resolve_mounts(proc_root, cfg);
        let config = super::scheduler_config(cfg, hw, &auto_mounts, ConfigGeneration(1));
        let display_jobs = config
            .jobs
            .iter()
            .filter(|spec| spec.timing == TimingClass::FastDisplay)
            .map(|spec| spec.id.clone())
            .collect();
        let mut scheduler = Scheduler::new();
        let mut pending = VecDeque::from(
            scheduler
                .handle(SchedulerEvent::Startup {
                    at,
                    config,
                    inventory_generation: InventoryGeneration(1),
                })
                .actions,
        );
        pending.extend(
            scheduler
                .handle(SchedulerEvent::TooltipPresented {
                    at,
                    presented: true,
                })
                .actions,
        );
        pending.extend(
            scheduler
                .handle(SchedulerEvent::SelectedPageChanged {
                    at,
                    page: selected_page,
                })
                .actions,
        );
        Self {
            scheduler,
            readings: DisplaySnapshot::default(),
            pending,
            display_jobs,
            sampled_once: false,
        }
    }

    pub(crate) fn sample(
        &mut self,
        mut owners: OwnerRefs<'_>,
        hw: &mut HardwareInventory,
        cfg: &Config,
        ctx: &mut CollectCtx<'_, '_>,
        mut timings: Option<&mut Timings>,
        mut observe_start: Option<&mut dyn FnMut(&JobId)>,
    ) -> DisplaySnapshot {
        let now = SchedulerTime::from_duration((ctx.clock)().monotonic);
        if self.sampled_once {
            for job in &self.display_jobs {
                self.pending.extend(
                    self.scheduler
                        .handle(SchedulerEvent::RefreshTriggered {
                            at: now,
                            job: job.clone(),
                            trigger: RefreshTrigger::Signal,
                        })
                        .actions,
                );
            }
        }
        self.pending.extend(
            self.scheduler
                .handle(SchedulerEvent::TimeAdvanced { at: now })
                .actions,
        );
        while let Some(action) = self.pending.pop_front() {
            match action {
                SchedulerAction::StartJob { ticket } => {
                    if !self.scheduler.is_current_ticket(&ticket) {
                        continue;
                    }
                    if let Some(observer) = observe_start.as_deref_mut() {
                        observer(&ticket.job);
                    }
                    let result = execute_scheduled_job(
                        &ticket,
                        owners.reborrow(),
                        hw,
                        cfg,
                        ctx,
                        &mut self.readings,
                        timings.as_deref_mut(),
                    );
                    let at = SchedulerTime::from_duration((ctx.clock)().monotonic);
                    self.pending.extend(
                        self.scheduler
                            .handle(SchedulerEvent::JobFinished {
                                at,
                                ticket,
                                completion: result.completion,
                                notification_ready: result.notification_ready,
                            })
                            .actions,
                    );
                }
                SchedulerAction::ResetCounterBaseline { job } => {
                    reset_counter_baseline(&job, owners.reborrow());
                }
                SchedulerAction::InvalidateJob { job, .. } => {
                    invalidate_scheduled_job(&job, owners.reborrow(), &mut self.readings);
                }
                SchedulerAction::CancelJob { ticket, .. } => {
                    self.pending.extend(
                        self.scheduler
                            .handle(SchedulerEvent::JobCancelled { at: now, ticket })
                            .actions,
                    );
                }
                SchedulerAction::PublishDisplay { .. }
                | SchedulerAction::EvaluateNotifications { .. }
                | SchedulerAction::RescanHardware { .. }
                | SchedulerAction::ApplyBackoff { .. }
                | SchedulerAction::ScheduleDeadline { .. }
                | SchedulerAction::Terminate { .. } => {}
            }
        }
        self.sampled_once = true;
        self.readings.assembled_at = (ctx.clock)();
        self.readings.clone()
    }
}

pub(crate) fn execute_scheduled_job(
    execution: &(impl ScheduledExecutionIdentity + ?Sized),
    mut owners: OwnerRefs<'_>,
    hw: &mut HardwareInventory,
    cfg: &Config,
    ctx: &mut CollectCtx<'_, '_>,
    readings: &mut DisplaySnapshot,
    mut timings: Option<&mut Timings>,
) -> JobExecution {
    let job = execution.job();
    let history_deadline = execution.history_deadline();
    let all_caps = super::catalog::configured_capabilities(cfg);
    reconcile(&mut owners, hw, cfg, &all_caps);
    let mut notifications = DisplaySnapshot::default();
    let completion = match job.kind {
        JobKind::Cpu => {
            let result = attempt_cpu(
                owners.cpu,
                ctx.proc_root,
                ctx.sys_root,
                hw,
                &all_caps,
                &mut *ctx.clock,
                &mut timings,
            );
            notifications.cpu_temp = current_value(&result.temperature).copied();
            notifications.load_average = current_value(&result.load_average).copied();
            let completion = completion(result.usage.status);
            readings.cpu_usage = sample_value(result.usage);
            readings.cpu_temp = sample_value(result.temperature);
            readings.cpu_freq_mhz = sample_value(result.frequency_mhz);
            readings.cpu_turbo = sample_value(result.turbo);
            readings.uptime_seconds = sample_value(result.uptime_seconds);
            readings.load_average = sample_value(result.load_average);
            completion
        }
        JobKind::CpuHistory => {
            if let Some(value) = sample_at_history_deadline(&owners.cpu.usage, history_deadline)
                .map(|sample| sample.value)
            {
                let captured_at = history_capture_time(history_deadline, ctx);
                cpu::append_cpu_history(owners.cpu, cfg, captured_at, value);
                readings.cpu_history.clone_from(&owners.cpu.cpu_history);
                CompletionKind::Captured
            } else {
                CompletionKind::Baseline
            }
        }
        JobKind::CpuCores => {
            let result = attempt_cpu_cores(owners.cpu, ctx.proc_root, (ctx.clock)(), &mut timings);
            let completion = completion(result.usage.status);
            readings.cpu_core_usage = sample_value(result.usage);
            completion
        }
        JobKind::CpuCoreHistory => {
            if let Some(value) =
                sample_at_history_deadline(&owners.cpu.core_usage, history_deadline)
                    .map(|sample| sample.value.clone())
            {
                let captured_at = history_capture_time(history_deadline, ctx);
                cpu::append_cpu_core_history(owners.cpu, cfg, captured_at, &value);
                readings.cpu_core_history = Some(owners.cpu.cpu_core_history.clone());
                CompletionKind::Captured
            } else {
                CompletionKind::Baseline
            }
        }
        JobKind::Memory => {
            let result = attempt_memory(
                owners.memory,
                ctx.proc_root,
                all_caps.contains(&Capability::SwapUsage),
                &mut *ctx.clock,
                &mut timings,
            );
            let completion = completion(result.usage.status);
            if let Some(sample) = result.usage.sample {
                readings.mem_usage = Some(sample.value.percent);
                readings.mem_used_gib = Some(sample.value.used_gib);
                readings.mem_total_gib = Some(sample.value.total_gib);
            }
            readings.swap_usage = sample_value(result.swap_usage);
            completion
        }
        JobKind::MemoryHistory => {
            if let Some(value) = sample_at_history_deadline(&owners.memory.usage, history_deadline)
                .map(|sample| sample.value.percent)
            {
                let captured_at = history_capture_time(history_deadline, ctx);
                memory::append_memory_history(owners.memory, cfg, captured_at, value);
                readings.mem_history.clone_from(&owners.memory.mem_history);
                CompletionKind::Captured
            } else {
                CompletionKind::Baseline
            }
        }
        JobKind::NetworkRate => {
            let Some(device) = source_device(job) else {
                return absent(notifications);
            };
            let result = attempt_network_speed(
                owners.network,
                ctx.sys_root,
                device,
                (ctx.clock)(),
                &mut timings,
            );
            let completion = completion(result.reading.status);
            let (up, down) = result.reading.sample.map_or((None, None), |sample| {
                (Some(sample.value.0), Some(sample.value.1))
            });
            readings.net_up_bps = up;
            readings.net_down_bps = down;
            completion
        }
        JobKind::NetworkIdentity => {
            let result = attempt_network_info(
                owners.network,
                ctx.sys_root,
                ctx.commands,
                &mut *ctx.clock,
                &mut timings,
            );
            let completion = completion(result.route_status);
            if result.route_status == AttemptStatus::Captured {
                let device = result
                    .reading
                    .sample
                    .as_ref()
                    .and_then(|sample| sample.value.device.clone());
                if hw.net_device != device {
                    hw.net_device = device;
                    owners.network.reset_rate();
                    owners.network.reset_history();
                    readings.net_up_bps = None;
                    readings.net_down_bps = None;
                    readings.net_up_history.clear();
                    readings.net_down_history.clear();
                }
            }
            if let Some(sample) = result.reading.sample {
                readings.net_device = sample.value.device.clone();
                readings.ip_address = sample.value.ip_address;
                readings.wifi_ssid = None;
                readings.wifi_signal_percent = None;
                if let Some(wifi) = result.wifi.sample
                    && Some(wifi.value.device.as_str()) == readings.net_device.as_deref()
                {
                    readings.wifi_ssid = wifi.value.ssid;
                    readings.wifi_signal_percent = wifi.value.signal_pct;
                }
            }
            completion
        }
        JobKind::NetworkHistory => {
            if let Some(value) = sample_at_history_deadline(&owners.network.rate, history_deadline)
                .map(|sample| sample.value)
            {
                let captured_at = history_capture_time(history_deadline, ctx);
                network::append_net_history(
                    owners.network,
                    cfg,
                    captured_at,
                    Some(value.0),
                    Some(value.1),
                );
                readings.net_up_history = owners.network.net_up_history().to_vec();
                readings.net_down_history = owners.network.net_down_history().to_vec();
                CompletionKind::Captured
            } else {
                CompletionKind::Baseline
            }
        }
        JobKind::DiskIo => {
            let Some(device) = source_device(job) else {
                return absent(notifications);
            };
            let result = attempt_disk_io(
                owners.disk,
                ctx.proc_root,
                device,
                (ctx.clock)(),
                &mut timings,
            );
            let completion = completion(result.reading.status);
            let (read, write) = result.reading.sample.map_or((None, None), |sample| {
                (Some(sample.value.0), Some(sample.value.1))
            });
            readings.disk_read_bps = read;
            readings.disk_write_bps = write;
            completion
        }
        JobKind::DiskUsage => {
            let SourceIdentity::Mount(path) = &job.source else {
                return absent(notifications);
            };
            let mount = path.to_string_lossy();
            let result = attempt_disk_usage(owners.disk, &mount, (ctx.clock)(), &mut timings);
            let completion = completion(result.reading.status);
            let value = result.reading.sample.map(|sample| DiskUsageReading {
                percent: sample.value.percent,
                used_gib: sample.value.used_gb,
                total_gib: sample.value.total_gb,
            });
            if result.reading.status == AttemptStatus::Captured {
                notifications.disk_usage.insert(result.mount.clone(), value);
            }
            readings.disk_usage.insert(result.mount, value);
            completion
        }
        JobKind::Smart => {
            let SourceIdentity::SmartDrive { label, object_path } = &job.source else {
                return absent(notifications);
            };
            let Some(drive) = hw.disk_smart_drives.get(label) else {
                return absent(notifications);
            };
            if &drive.object_path != object_path {
                return absent(notifications);
            }
            let result =
                attempt_smart(owners.disk, ctx.dbus, label, drive, (ctx.clock)().monotonic);
            let completion = completion(result.reading.status);
            let value = result.reading.sample.map(|sample| sample.value);
            if result.reading.status == AttemptStatus::Captured {
                notifications.disk_smart.insert(label.clone(), value);
            }
            readings.disk_smart.insert(label.clone(), value);
            completion
        }
        JobKind::DiskTemperature | JobKind::FanSpeed => {
            let SourceIdentity::NamedPath { name, path } = &job.source else {
                return absent(notifications);
            };
            if job.kind == JobKind::DiskTemperature {
                let result =
                    attempt_disk_temperature(owners.disk, name, path, (ctx.clock)().monotonic);
                let completion = completion(result.reading.status);
                let value = result.reading.sample.map(|sample| sample.value);
                if result.reading.status == AttemptStatus::Captured {
                    notifications.hd_temps.insert(name.clone(), value);
                }
                readings.hd_temps.insert(name.clone(), value);
                completion
            } else {
                let result = attempt_fan_speed(owners.disk, name, path, (ctx.clock)().monotonic);
                let completion = completion(result.reading.status);
                readings.fan_speeds.insert(
                    name.clone(),
                    result.reading.sample.map(|sample| sample.value),
                );
                completion
            }
        }
        JobKind::SystemBattery => {
            let SourceIdentity::Battery(id) = &job.source else {
                return absent(notifications);
            };
            let result = attempt_system_battery(
                owners.power,
                ctx.dbus,
                id,
                ctx.sys_root,
                (ctx.clock)(),
                &mut timings,
            );
            let completion = completion(result.reading.status);
            readings.battery_sys.retain(|battery| battery.id != *id);
            if let Some(sample) = result.reading.sample {
                if result.reading.status == AttemptStatus::Captured {
                    notifications.battery_sys.push(sample.value.clone());
                }
                readings.battery_sys.push(sample.value);
            }
            completion
        }
        JobKind::PeripheralBattery => execute_peripheral(
            job,
            owners.power,
            cfg,
            ctx,
            readings,
            &mut notifications,
            &mut timings,
        ),
        JobKind::AmdFast | JobKind::AmdSlow => gpu::execute_amd(
            job,
            execution.metrics(),
            owners.amd_gpu,
            hw,
            cfg,
            ctx,
            readings,
            &mut notifications,
        ),
        JobKind::NvidiaNvml
        | JobKind::NvidiaFallback
        | JobKind::IntelFrequency
        | JobKind::IntelUsage
        | JobKind::GpuHistory => gpu::execute(
            job,
            owners.reborrow(),
            hw,
            cfg,
            ctx,
            readings,
            &mut notifications,
            &mut timings,
            history_deadline,
        ),
        JobKind::PanelProcesses => {
            let result = attempt_process(owners.process, ctx.proc_root, (ctx.clock)());
            let completion = completion(result.reading.status);
            if let Some(sample) = result.reading.sample {
                readings.top_process = Some(sample.value.summary);
                readings.top_process_full = Some(sample.value.full);
            }
            completion
        }
        JobKind::PageProcesses => {
            let result = super::process::read_top_process_page_attempt(
                ctx.proc_root,
                owners.process,
                (ctx.clock)(),
            );
            readings.top_process_full = result.sample.map(|sample| sample.value);
            match result.status {
                ProcessPageStatus::Captured => CompletionKind::Captured,
                ProcessPageStatus::Baseline => CompletionKind::Baseline,
                ProcessPageStatus::Empty => CompletionKind::ConfirmedAbsent,
                ProcessPageStatus::Failed => CompletionKind::Failed,
            }
        }
        JobKind::Brightness | JobKind::UpdatesFile | JobKind::ServerFile => {
            let capability = match job.kind {
                JobKind::Brightness => Capability::ScreenBrightness,
                JobKind::UpdatesFile => Capability::SystemUpdates,
                JobKind::ServerFile => Capability::ServerCheck,
                _ => unreachable!(),
            };
            let result = attempt_external(
                owners.external,
                ctx.sys_root,
                cfg,
                &BTreeSet::from([capability]),
                &mut *ctx.clock,
                &mut timings,
            );
            match job.kind {
                JobKind::Brightness => {
                    let completion = completion(result.brightness.status);
                    readings.screen_brightness = sample_value(result.brightness);
                    completion
                }
                JobKind::UpdatesFile => {
                    let completion = completion(result.updates.status);
                    readings.system_updates = sample_value(result.updates);
                    completion
                }
                JobKind::ServerFile => {
                    let completion = completion(result.server.status);
                    if result.server.status == AttemptStatus::Captured {
                        notifications.server_ok = current_value(&result.server).copied();
                    }
                    readings.server_ok = sample_value(result.server);
                    completion
                }
                _ => unreachable!(),
            }
        }
        JobKind::PageCommand
        | JobKind::PageRender
        | JobKind::HardwareDiscovery
        | JobKind::MountInventory => CompletionKind::ConfirmedAbsent,
    };
    notifications.assembled_at = (ctx.clock)();
    JobExecution {
        completion,
        notification_ready: has_notification_sample(&notifications),
        notifications,
    }
}

pub(crate) fn reset_counter_baseline(job: &JobId, owners: OwnerRefs<'_>) {
    match job.kind {
        JobKind::Cpu => owners.cpu.cpu_prev_times.clear(),
        JobKind::CpuCores => owners.cpu.cpu_core_prev_times.clear(),
        JobKind::NetworkRate => owners.network.invalidate_rate_baseline(),
        JobKind::DiskIo => owners.disk.invalidate_io_baseline(),
        JobKind::IntelUsage => {
            owners.intel_gpu.engine_prev.clear();
            owners.intel_gpu.prev_sample_at = None;
            owners.intel_gpu.usage_needs_comparable = true;
        }
        JobKind::PanelProcesses => {
            owners.process.proc_prev_times.clear();
            owners.process.proc_prev_sample_at = None;
        }
        JobKind::PageProcesses => {
            owners.process.page_proc_prev_times.clear();
            owners.process.page_proc_prev_sample_at = None;
        }
        JobKind::CpuHistory
        | JobKind::CpuCoreHistory
        | JobKind::Memory
        | JobKind::MemoryHistory
        | JobKind::NetworkIdentity
        | JobKind::NetworkHistory
        | JobKind::DiskUsage
        | JobKind::Smart
        | JobKind::DiskTemperature
        | JobKind::FanSpeed
        | JobKind::SystemBattery
        | JobKind::PeripheralBattery
        | JobKind::AmdFast
        | JobKind::AmdSlow
        | JobKind::NvidiaNvml
        | JobKind::NvidiaFallback
        | JobKind::IntelFrequency
        | JobKind::GpuHistory
        | JobKind::Brightness
        | JobKind::UpdatesFile
        | JobKind::ServerFile
        | JobKind::PageCommand
        | JobKind::PageRender
        | JobKind::HardwareDiscovery
        | JobKind::MountInventory => {}
    }
}

fn reconcile(
    owners: &mut OwnerRefs<'_>,
    hw: &HardwareInventory,
    cfg: &Config,
    caps: &BTreeSet<Capability>,
) {
    owners
        .cpu
        .reconcile_core_page(cfg.pages.order.iter().any(|page| page == "cpu_cores"));
    owners
        .process
        .reconcile_panel(caps.contains(&Capability::TopProcess));
    owners.network.reconcile_sources(
        hw.net_device.as_deref(),
        caps.contains(&Capability::NetworkSpeed),
        caps.contains(&Capability::NetworkInfo),
        cfg.pages.order.iter().any(|page| page == "graphs"),
    );
    owners.disk.reconcile_sources(hw, cfg, caps);
    owners.power.reconcile_sources(hw, cfg, caps);
    owners
        .nvidia
        .reconcile_source(caps.contains(&Capability::GpuNvidia) && hw.has_nvidia);
    gpu::reconcile_amd(owners.amd_gpu, hw, caps);
    owners.intel_gpu.reconcile_sources(
        hw.intel_gpu_pci.as_deref(),
        hw.intel_gpu_freq_path.as_ref(),
        caps.contains(&Capability::GpuIntelUsage) || caps.contains(&Capability::GpuIntelDecoder),
        caps.contains(&Capability::GpuIntelFrequency),
    );
    owners.external.reconcile_sources(
        cfg,
        hw,
        caps.contains(&Capability::SystemUpdates),
        caps.contains(&Capability::ServerCheck),
        caps.contains(&Capability::ScreenBrightness),
    );
}

fn sample_at_history_deadline<T>(
    sample: &RetainedMetricSample<T>,
    deadline: Option<HistoryDeadline>,
) -> Option<&MetricSample<T>> {
    deadline.map_or(sample.latest.as_ref(), |deadline| {
        sample.sample_at_or_before(deadline.at().duration())
    })
}

fn history_capture_time(
    deadline: Option<HistoryDeadline>,
    ctx: &mut CollectCtx<'_, '_>,
) -> Duration {
    deadline.map_or_else(
        || (ctx.clock)().monotonic,
        |deadline| deadline.at().duration(),
    )
}

fn current_value<T>(result: &AttemptResult<T>) -> Option<&T> {
    (result.status == AttemptStatus::Captured)
        .then_some(result.sample.as_ref().map(|sample| &sample.value))
        .flatten()
}

fn sample_value<T>(result: AttemptResult<T>) -> Option<T> {
    result.sample.map(|sample| sample.value)
}

fn completion(status: AttemptStatus) -> CompletionKind {
    match status {
        AttemptStatus::Captured | AttemptStatus::Cached => CompletionKind::Captured,
        AttemptStatus::Baseline => CompletionKind::Baseline,
        AttemptStatus::Absent => CompletionKind::ConfirmedAbsent,
        AttemptStatus::Failed => CompletionKind::Failed,
    }
}

fn source_device(job: &JobId) -> Option<&str> {
    match &job.source {
        SourceIdentity::Device(device) => Some(device),
        SourceIdentity::Singleton
        | SourceIdentity::CpuSources { .. }
        | SourceIdentity::Path(_)
        | SourceIdentity::Mount(_)
        | SourceIdentity::NamedPath { .. }
        | SourceIdentity::SmartDrive { .. }
        | SourceIdentity::Battery(_)
        | SourceIdentity::Peripheral { .. }
        | SourceIdentity::Inventory(_)
        | SourceIdentity::Page(_) => None,
    }
}

fn absent(notifications: DisplaySnapshot) -> JobExecution {
    JobExecution {
        completion: CompletionKind::ConfirmedAbsent,
        notification_ready: false,
        notifications,
    }
}

fn has_notification_sample(readings: &DisplaySnapshot) -> bool {
    readings.cpu_temp.is_some()
        || readings.load_average.is_some()
        || !readings.disk_usage.is_empty()
        || !readings.disk_smart.is_empty()
        || !readings.hd_temps.is_empty()
        || !readings.battery_sys.is_empty()
        || readings.battery_mouse.is_some()
        || readings.battery_kbd.is_some()
        || readings.gpu_temp.is_some()
        || readings.gpu_amd_temp.is_some()
        || readings.server_ok.is_some()
}
