use super::*;

impl<C: CommandRunner, D: DbusFacade> WorkerState<C, D> {
    pub(super) fn execute(
        &mut self,
        input: JobInput,
        roots: &FilesystemRoots,
        clock: &ProductionClock,
    ) -> JobCompletion {
        self.pages_changed(&input.active);
        let ticket = input.ticket;
        let style_generation = input.style_generation;
        let render_generation = input.render_generation;
        let mut hw = input.hw;
        let mut readings = input.readings;
        let mut resolved_mounts = input.resolved_mounts;
        let mut rendered_page = None;
        let (completion, notification_ready, notifications) = match ticket.job.kind {
            JobKind::PageCommand => {
                self.execute_page_command(&ticket, &input.active, clock, &mut rendered_page)
            }
            JobKind::PageRender => {
                rendered_page = Some(render_page_cached(
                    &input.cfg,
                    &hw,
                    &readings,
                    &input.css,
                    &input.active,
                    input.selected_index,
                    &self.command_cache,
                    &roots.proc_root,
                ));
                (CompletionKind::Captured, false, DisplaySnapshot::default())
            }
            JobKind::GpuHistory => {
                match (&ticket.job.source, input.gpu_history_point.as_ref()) {
                    (SourceIdentity::Device(source), point) if source == "nvidia" => {
                        readings.gpu_usage = point.and_then(|sample| sample.value.0);
                        readings.gpu_dec = point.and_then(|sample| sample.value.1);
                    }
                    (SourceIdentity::Device(source), point) if source.starts_with("amd:") => {
                        readings.gpu_amd_usage = point.and_then(|sample| sample.value.0);
                        readings.gpu_amd_codec_usage = point.and_then(|sample| sample.value.1);
                    }
                    (SourceIdentity::Device(source), point) if source.starts_with("intel:") => {
                        readings.gpu_intel_usage = point.and_then(|sample| sample.value.0);
                        readings.gpu_intel_dec_usage = point.and_then(|sample| sample.value.1);
                    }
                    _ => {}
                }
                let decoder_outcome = if matches!(&ticket.job.source, SourceIdentity::Device(source) if source.starts_with("amd:"))
                {
                    if hw
                        .amd_gpu
                        .as_ref()
                        .is_none_or(|source| source.codec_usage_path.is_none())
                    {
                        gpu_history::DecoderOutcome::ConfirmedAbsent
                    } else {
                        input
                            .gpu_history_point
                            .as_ref()
                            .map_or(gpu_history::DecoderOutcome::Unmeasured, |sample| {
                                sample.value.2
                            })
                    }
                } else {
                    input
                        .gpu_history_point
                        .as_ref()
                        .map_or(input.gpu_decoder_outcome, |sample| sample.value.2)
                };
                let captured_at = ticket.history_deadline.map_or_else(
                    || clock.snapshot(),
                    |deadline| ClockSnapshot {
                        monotonic: deadline.at().duration(),
                        ..ClockSnapshot::default()
                    },
                );
                let result = self.owners.gpu_history.sample(
                    &input.cfg,
                    &hw,
                    &readings,
                    decoder_outcome,
                    captured_at,
                    true,
                );
                let completion = if let Some(result) = result {
                    readings.gpu_usage_history =
                        result.usage.map(|sample| sample.value).unwrap_or_default();
                    readings.gpu_dec_history = result
                        .decoder
                        .map(|sample| sample.value)
                        .unwrap_or_default();
                    CompletionKind::Captured
                } else {
                    CompletionKind::ConfirmedAbsent
                };
                (completion, false, DisplaySnapshot::default())
            }
            JobKind::HardwareDiscovery => {
                let completion = if let SourceIdentity::Inventory(family) = ticket.job.source {
                    match reconcile_inventory_family(
                        family,
                        &mut hw,
                        &roots.sys_root,
                        &roots.proc_root,
                        &input.cfg,
                        &mut self.dbus,
                        &mut self.commands,
                    ) {
                        ReconciliationOutcome::Captured => CompletionKind::Captured,
                        ReconciliationOutcome::Failed => CompletionKind::Failed,
                    }
                } else {
                    CompletionKind::ConfirmedAbsent
                };
                (completion, false, DisplaySnapshot::default())
            }
            JobKind::MountInventory => {
                let completion = match disk::try_resolve_mounts(&roots.proc_root, &input.cfg) {
                    Ok(mounts) => {
                        resolved_mounts = mounts;
                        CompletionKind::Captured
                    }
                    Err(_) => CompletionKind::Failed,
                };
                (completion, false, DisplaySnapshot::default())
            }
            _ => {
                let mut capture_clock = || clock.snapshot();
                let mut context = CollectCtx::new(
                    roots,
                    &mut self.commands,
                    &mut self.dbus,
                    &mut capture_clock,
                );
                context.bolt = self
                    .bolt
                    .as_mut()
                    .map(|bolt| bolt as &mut dyn power::BoltBatteryFacade);
                #[cfg(feature = "nvml")]
                {
                    context.nvml = self
                        .nvml
                        .as_mut()
                        .map(|nvml| nvml as &mut dyn gpu_nvidia::NvmlFacade);
                }
                let result = crate::sensors::execute_scheduled_job(
                    &ticket,
                    self.owners.refs(),
                    &mut hw,
                    &input.cfg,
                    &mut context,
                    &mut readings,
                    None,
                );
                (
                    result.completion,
                    result.notification_ready,
                    result.notifications,
                )
            }
        };
        let decoder_outcome = if ticket.job.kind == JobKind::AmdFast {
            Some(self.owners.amd_gpu.codec_outcome())
        } else {
            job_decoder_outcome(
                ticket.job.kind,
                completion,
                readings.gpu_dec,
                readings.gpu_intel_dec_usage,
            )
        };
        let gpu_history_point = match ticket.job.kind {
            JobKind::NvidiaNvml | JobKind::NvidiaFallback => self
                .owners
                .nvidia
                .latest_history_point()
                .map(|sample| (SourceIdentity::Device(String::from("nvidia")), sample)),
            JobKind::AmdFast => self
                .owners
                .amd_gpu
                .latest_history_point()
                .map(|sample| (ticket.job.source.clone(), sample)),
            JobKind::IntelUsage => {
                let source = match &ticket.job.source {
                    SourceIdentity::Device(pci) => SourceIdentity::Device(format!("intel:{pci}")),
                    source => source.clone(),
                };
                self.owners
                    .intel_gpu
                    .latest_history_point()
                    .map(|sample| (source, sample))
            }
            _ => None,
        }
        .map(|(source, sample)| {
            let outcome = decoder_outcome.unwrap_or_else(|| {
                sample.value.1.map_or(
                    gpu_history::DecoderOutcome::Unmeasured,
                    gpu_history::DecoderOutcome::Value,
                )
            });
            (
                source,
                MetricSample::new(
                    (sample.value.0, sample.value.1, outcome),
                    sample.captured_at,
                ),
            )
        });
        JobCompletion {
            ticket,
            completion,
            readings,
            notifications,
            notification_ready,
            hw,
            resolved_mounts,
            command_cache: (self.owner == OwnerId::Page).then(|| self.command_cache.clone()),
            rendered_page,
            style_generation,
            render_generation,
            decoder_outcome,
            gpu_history_point,
            decision: None,
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
#[path = "execution/tests.rs"]
mod tests;
