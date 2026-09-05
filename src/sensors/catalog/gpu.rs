use super::*;
use crate::sensors::{gpu_intel, gpu_nvidia};

impl Catalog<'_> {
    pub(super) fn nvidia_jobs(&mut self) -> Vec<JobId> {
        if !self.hw.has_nvidia {
            return Vec::new();
        }
        vec![
            self.fast_singleton(OwnerId::Nvidia, JobKind::NvidiaNvml),
            self.periodic_singleton(
                OwnerId::Nvidia,
                JobKind::NvidiaFallback,
                gpu_nvidia::GPU_CACHE_TTL,
            ),
        ]
    }

    pub(super) fn intel_frequency_jobs(&mut self) -> Vec<JobId> {
        let Some(path) = self.hw.intel_gpu_freq_path.clone() else {
            return Vec::new();
        };
        let id = JobId::with_source(
            OwnerId::IntelGpu,
            JobKind::IntelFrequency,
            SourceIdentity::Path(path),
        );
        vec![self.insert(JobSpec::fast(id, self.cfg.display.poll_interval.duration()))]
    }

    pub(super) fn intel_usage_jobs(&mut self) -> Vec<JobId> {
        let Some(pci) = self.hw.intel_gpu_pci.clone() else {
            return Vec::new();
        };
        let id = JobId::with_source(
            OwnerId::IntelGpu,
            JobKind::IntelUsage,
            SourceIdentity::Device(pci),
        );
        let mut spec = JobSpec::periodic(id, gpu_intel::INTEL_GPU_USAGE_TTL);
        spec.counter = true;
        vec![self.insert(spec)]
    }
}

impl Catalog<'_> {
    pub(super) fn amd_jobs(&mut self, metric: Metric) -> Vec<JobId> {
        let Some(source) = self.hw.amd_gpu.as_ref() else {
            return Vec::new();
        };
        if !crate::sensors::gpu_amd::supported_metrics(source).contains(&metric) {
            return Vec::new();
        }
        let fast = crate::sensors::gpu_amd::FAST_METRICS.contains(&metric);
        let id = JobId::with_source(
            OwnerId::AmdGpu,
            if fast {
                JobKind::AmdFast
            } else {
                JobKind::AmdSlow
            },
            SourceIdentity::Device(format!("amd:{}", source.pci_identity)),
        );
        let spec = if fast {
            JobSpec::fast(id, self.cfg.display.poll_interval.duration())
        } else {
            JobSpec::periodic(id, Duration::from_secs(30))
        };
        vec![self.insert(spec)]
    }

    pub(super) fn finish_amd_jobs(&mut self) {
        use crate::sensors::gpu_amd::{
            FAST_METRICS, SLOW_METRICS, source_for_metrics, supported_metrics,
        };
        let Some(source) = self.hw.amd_gpu.as_ref() else {
            return;
        };
        let surface_metrics = |surface: &Surface| -> BTreeSet<Metric> {
            surface
                .sections
                .iter()
                .flat_map(|section| &section.items)
                .filter_map(|token| token.parse::<ItemToken>().ok())
                .map(ItemToken::metric)
                .collect()
        };
        let mut hidden = surface_metrics(&self.cfg.panel);
        let tooltip = surface_metrics(&self.cfg.tooltip);
        if self.cfg.notifications.gpu_amd_temp {
            hidden.insert(Metric::GpuAmdTemp);
        }
        if !self.hw.has_nvidia && self.cfg.pages.order.iter().any(|page| page == "graphs") {
            hidden.extend([Metric::GpuAmdUsage, Metric::GpuAmdCodecUsage]);
        }
        let supported = supported_metrics(source);
        for spec in self
            .specs
            .values_mut()
            .filter(|spec| spec.id.owner == OwnerId::AmdGpu)
        {
            let group = if spec.id.kind == JobKind::AmdFast {
                FAST_METRICS
            } else {
                SLOW_METRICS
            };
            let filter = |metrics: &BTreeSet<Metric>| {
                metrics
                    .iter()
                    .copied()
                    .filter(|metric| group.contains(metric) && supported.contains(metric))
                    .collect::<BTreeSet<_>>()
            };
            let hidden = filter(&hidden);
            let tooltip = filter(&tooltip);
            let metrics = hidden.union(&tooltip).copied().collect();
            spec.amd = Some(Box::new(crate::scheduler::AmdJob {
                source: source_for_metrics(source, &metrics),
                hidden,
                tooltip,
            }));
        }
    }
}

impl Catalog<'_> {
    pub(super) fn graph_jobs(&mut self) -> BTreeSet<JobId> {
        let cpu = self.cpu_job();
        let memory = self.memory_job();
        let mut jobs = BTreeSet::from([
            cpu.clone(),
            self.cpu_history(cpu),
            memory.clone(),
            self.memory_history(memory),
        ]);
        if let Some(network) = self.network_rate_job() {
            jobs.insert(network.clone());
            jobs.insert(self.network_history(network));
        }
        let gpu_sources = if self.hw.has_nvidia {
            self.nvidia_jobs()
        } else if self.hw.amd_gpu.is_some() {
            let mut jobs = self.amd_jobs(Metric::GpuAmdUsage);
            jobs.extend(self.amd_jobs(Metric::GpuAmdCodecUsage));
            jobs
        } else {
            self.intel_usage_jobs()
        };
        jobs.extend(gpu_sources);
        let gpu_source = crate::sensors::gpu_history::selected_source(self.hw);
        if let Some(source) = gpu_source {
            jobs.insert(self.insert(JobSpec::source_history(
                JobId::with_source(
                    OwnerId::GpuHistory,
                    JobKind::GpuHistory,
                    SourceIdentity::Device(source),
                ),
                self.cfg.display.history_interval.duration(),
            )));
        }
        jobs
    }
}
