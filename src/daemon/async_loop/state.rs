use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::{
    Config, apply_canonical_width, default_config_path, load_config, machine_source_paths,
    resolve_style,
};
use crate::domain::boundary::{ClockSnapshot, FilesystemRoots};
use crate::domain::readings::{
    DisplaySnapshot, HardwareInventory, InventoryFamily, RetainedMetricSample,
};
use crate::domain::state::NotificationState;
use crate::error::Result;
use crate::page_commands::{Page, PageCommandCache, build_pages, pager_html, title_html};
use crate::render::{PageFormatter, PanelFormatter};
use crate::scheduler::{
    ConfigGeneration, InventoryGeneration, OwnerId, PageId, PublicationId, PublishReason, RunId,
    Scheduler, SchedulerEvent, SchedulerTime, Transition,
};
use crate::sensors::scheduler_config;

use super::super::{
    DaemonPaths, clock_unix, log_boot_ready, mtime, page_index, read_css, render_page_cached,
    write_atomic,
};
use super::worker::{GpuHistoryPoint, GpuHistoryValue, JobCompletion};

pub(super) struct RuntimeState {
    pub(super) cfg: Config,
    pub(super) hw: HardwareInventory,
    pub(super) active: Vec<Page>,
    pub(super) readings: DisplaySnapshot,
    pub(super) notifications: NotificationState,
    pub(super) notification_samples: BTreeMap<RunId, DisplaySnapshot>,
    pub(super) command_cache: PageCommandCache,
    pub(super) config_generation: ConfigGeneration,
    pub(super) inventory_generation: InventoryGeneration,
    pub(super) css: String,
    pub(super) style_generation: u64,
    pub(super) render_generation: u64,
    pub(super) css_path: PathBuf,
    pub(super) overlay_path: Option<PathBuf>,
    pub(super) kde_stamp: Option<SystemTime>,
    pub(super) css_stamp: Option<SystemTime>,
    pub(super) overlay_stamp: Option<SystemTime>,
    pub(super) theme_reconciliation_pending: bool,
    pub(super) panel_html: Option<String>,
    pub(super) tooltip_html: Option<String>,
    pub(super) rendered_graph: Option<(PageId, u64, String)>,
    pub(super) first_paint_published: bool,
    pub(super) resolved_mounts: Vec<String>,
    pub(super) boot_pending: BTreeSet<&'static str>,
    pub(super) last_committed: BTreeMap<OwnerId, RunId>,
    pub(super) nvidia_decoder_outcome: Option<(
        crate::scheduler::SourceIdentity,
        crate::sensors::gpu_history::DecoderOutcome,
    )>,
    pub(super) intel_decoder_outcome: Option<(
        crate::scheduler::SourceIdentity,
        crate::sensors::gpu_history::DecoderOutcome,
    )>,
    pub(super) gpu_history_samples:
        BTreeMap<crate::scheduler::SourceIdentity, RetainedMetricSample<GpuHistoryValue>>,
    pub(super) terminate: bool,
    pub(super) critical_component: Option<&'static str>,
    pub(super) deferred_first_paint: Option<crate::scheduler::SchedulerAction>,
    pub(super) deferred_first_paint_jobs: BTreeSet<crate::scheduler::JobId>,
    pub(super) completed_jobs: BTreeSet<crate::scheduler::JobId>,
}

impl RuntimeState {
    pub(super) fn new(
        paths: &DaemonPaths,
        cfg: Config,
        hw: HardwareInventory,
        active: Vec<Page>,
    ) -> Self {
        let light = super::super::kdeglobals_background(&paths.kdeglobals)
            .is_some_and(super::super::is_light_rgb);
        let css_path = resolve_style(if light {
            "style-light.css"
        } else {
            "style-dark.css"
        });
        let overlay_path = cfg
            .display
            .overlay
            .then(|| resolve_style("style-overlay.css"));
        let css = read_css(&css_path, overlay_path.as_deref());
        Self {
            kde_stamp: mtime(&paths.kdeglobals),
            css_stamp: mtime(&css_path),
            overlay_stamp: overlay_path.as_deref().and_then(mtime),
            theme_reconciliation_pending: true,
            cfg,
            hw,
            active,
            readings: DisplaySnapshot::default(),
            notifications: NotificationState::default(),
            notification_samples: BTreeMap::new(),
            command_cache: PageCommandCache::new(),
            config_generation: ConfigGeneration(1),
            inventory_generation: InventoryGeneration(1),
            css,
            style_generation: 1,
            render_generation: 1,
            css_path,
            overlay_path,
            panel_html: None,
            tooltip_html: None,
            rendered_graph: None,
            first_paint_published: false,
            resolved_mounts: vec![String::from("/")],
            boot_pending: BTreeSet::from([
                "battery_sys",
                "battery_mouse",
                "battery_kbd",
                "hd_temps",
                "fan_speeds",
                "gpu_nvidia",
                "gpu_intel",
                "system_updates",
                "server_check",
                "top_process",
            ]),
            last_committed: BTreeMap::new(),
            nvidia_decoder_outcome: None,
            intel_decoder_outcome: None,
            gpu_history_samples: BTreeMap::new(),
            terminate: false,
            critical_component: None,
            deferred_first_paint: None,
            deferred_first_paint_jobs: BTreeSet::new(),
            completed_jobs: BTreeSet::new(),
        }
    }

    pub(super) fn scheduler_config(&self) -> crate::scheduler::SchedulerConfig {
        scheduler_config(
            &self.cfg,
            &self.hw,
            &self.resolved_mounts,
            self.config_generation,
        )
    }

    pub(super) fn selected_page(&self, paths: &DaemonPaths) -> (usize, PageId) {
        let index = page_index(paths, self.active.len());
        (index, PageId::from_id(self.active[index].id))
    }

    pub(super) fn can_commit(&self, scheduler: &Scheduler, completion: &JobCompletion) -> bool {
        scheduler.is_current_ticket(&completion.ticket)
            && self.render_input_current(completion)
            && self
                .last_committed
                .get(&completion.ticket.job.owner)
                .is_none_or(|run| *run < completion.ticket.run_id)
    }

    pub(super) fn render_input_current(&self, completion: &JobCompletion) -> bool {
        completion.ticket.job.kind != crate::scheduler::JobKind::PageRender
            || (completion.style_generation == self.style_generation
                && completion.render_generation == self.render_generation)
    }

    pub(super) fn decoder_outcome_for(
        &self,
        job: &crate::scheduler::JobId,
    ) -> crate::sensors::gpu_history::DecoderOutcome {
        let retained = match &job.source {
            crate::scheduler::SourceIdentity::Device(source) if source == "nvidia" => {
                self.nvidia_decoder_outcome.as_ref()
            }
            crate::scheduler::SourceIdentity::Device(source) if source.starts_with("intel:") => {
                self.intel_decoder_outcome.as_ref()
            }
            _ => None,
        };
        retained.filter(|(source, _)| source == &job.source).map_or(
            crate::sensors::gpu_history::DecoderOutcome::Unmeasured,
            |(_, outcome)| *outcome,
        )
    }

    pub(super) fn invalidate_decoder(&mut self, job: &crate::scheduler::JobId) {
        match job.owner {
            OwnerId::Nvidia => self.nvidia_decoder_outcome = None,
            OwnerId::IntelGpu => self.intel_decoder_outcome = None,
            OwnerId::GpuHistory => match &job.source {
                crate::scheduler::SourceIdentity::Device(source) if source == "nvidia" => {
                    self.nvidia_decoder_outcome = None;
                }
                crate::scheduler::SourceIdentity::Device(source)
                    if source.starts_with("intel:") =>
                {
                    self.intel_decoder_outcome = None;
                }
                _ => {}
            },
            _ => {}
        }
        match &job.source {
            crate::scheduler::SourceIdentity::Device(source) if source == "nvidia" => {
                self.gpu_history_samples
                    .remove(&crate::scheduler::SourceIdentity::Device(String::from(
                        "nvidia",
                    )));
            }
            crate::scheduler::SourceIdentity::Device(source) if source.starts_with("intel:") => {
                self.gpu_history_samples.remove(&job.source);
            }
            _ => {}
        }
    }

    pub(super) fn refresh_deferred_first_paint_jobs(&mut self) {
        if self.deferred_first_paint.is_none() {
            return;
        }
        self.deferred_first_paint_jobs = self
            .scheduler_config()
            .jobs
            .into_iter()
            .filter(|spec| spec.startup_panel && !self.completed_jobs.contains(&spec.id))
            .map(|spec| spec.id)
            .collect();
    }

    pub(super) fn gpu_history_point_for(
        &self,
        job: &crate::scheduler::JobTicket,
    ) -> Option<GpuHistoryPoint> {
        let samples = self.gpu_history_samples.get(&job.job.source)?;
        job.history_deadline.map_or_else(
            || samples.latest.clone(),
            |deadline| {
                samples
                    .sample_at_or_before(deadline.at().duration())
                    .cloned()
            },
        )
    }

    pub(super) fn commit(&mut self, completion: &JobCompletion) -> bool {
        let owner = completion.ticket.job.owner;
        apply_owner_readings(owner, &completion.readings, &mut self.readings);
        if let Some(outcome) = completion.decoder_outcome {
            let source = match owner {
                OwnerId::Nvidia => crate::scheduler::SourceIdentity::Device(String::from("nvidia")),
                OwnerId::IntelGpu => match &completion.ticket.job.source {
                    crate::scheduler::SourceIdentity::Device(pci) => {
                        crate::scheduler::SourceIdentity::Device(format!("intel:{pci}"))
                    }
                    source => source.clone(),
                },
                _ => completion.ticket.job.source.clone(),
            };
            let retained = (source, outcome);
            match owner {
                OwnerId::Nvidia => self.nvidia_decoder_outcome = Some(retained),
                OwnerId::IntelGpu => self.intel_decoder_outcome = Some(retained),
                _ => {}
            }
        }
        if let Some((source, sample)) = &completion.gpu_history_point {
            let retained = self.gpu_history_samples.entry(source.clone()).or_default();
            if retained
                .latest
                .as_ref()
                .is_none_or(|latest| latest.captured_at < sample.captured_at)
            {
                retained.record_value(sample.value, sample.captured_at);
            }
        }
        if let Some(cache) = &completion.command_cache {
            self.command_cache.clone_from(cache);
        }
        if let Some(html) = &completion.rendered_page {
            let page = match &completion.ticket.job.source {
                crate::scheduler::SourceIdentity::Page(page) => page.clone(),
                _ => PageId::Graphs,
            };
            self.rendered_graph = Some((page, completion.render_generation, html.clone()));
        }
        let before_hw = self.hw.clone();
        let before_mounts = self.resolved_mounts.clone();
        apply_inventory_completion(completion, &mut self.hw, &mut self.resolved_mounts);
        let inventory_changed = self.hw != before_hw || self.resolved_mounts != before_mounts;
        self.last_committed.insert(owner, completion.ticket.run_id);
        if owner != OwnerId::Page {
            self.render_generation = self.render_generation.saturating_add(1);
        }
        inventory_changed
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn publish(
        &mut self,
        publication: PublicationId,
        reason: PublishReason,
        panel: bool,
        tooltip: bool,
        scheduler: &mut Scheduler,
        roots: &FilesystemRoots,
        paths: &DaemonPaths,
        snapshot: ClockSnapshot,
        boot: ClockSnapshot,
    ) -> Result<Option<Transition>> {
        self.readings.assembled_at = snapshot;
        if matches!(
            reason,
            PublishReason::FirstPaintReady | PublishReason::FirstPaintTimeout
        ) {
            self.first_paint_published = true;
        }
        let mut acknowledgement = None;
        if panel {
            let width = PanelFormatter::new(&self.cfg, &self.hw).canonical_width(&self.readings);
            let previous_width = self.cfg.display.tooltip_width;
            apply_canonical_width(&mut self.cfg, i32::try_from(width).unwrap_or(i32::MAX));
            if self.cfg.display.tooltip_width != previous_width {
                self.rendered_graph = None;
                self.render_generation = self.render_generation.saturating_add(1);
            }
            let html = PanelFormatter::with_now_unix(&self.cfg, &self.hw, clock_unix(snapshot))
                .format_panel(&self.readings, &self.css);
            if self.panel_html.as_ref() != Some(&html) {
                write_atomic(&paths.panel, &html)?;
                self.panel_html = Some(html);
            }
            log_boot_ready(
                &self.readings,
                &mut self.boot_pending,
                boot.monotonic,
                snapshot.monotonic,
            );
            acknowledgement = Some(scheduler.handle(SchedulerEvent::PanelPublished {
                at: SchedulerTime::from_duration(snapshot.monotonic),
                publication,
            }));
        }
        if tooltip {
            self.publish_tooltip(roots, paths)?;
        }
        Ok(acknowledgement)
    }

    fn publish_tooltip(&mut self, roots: &FilesystemRoots, paths: &DaemonPaths) -> Result<()> {
        let (index, selected) = self.selected_page(paths);
        let html = if selected == PageId::Graphs {
            self.rendered_graph
                .as_ref()
                .filter(|(page, generation, _)| {
                    *page == selected && *generation == self.render_generation
                })
                .map(|(_, _, html)| html.clone())
                .or_else(|| Some(self.pending_page(index)))
        } else {
            Some(render_page_cached(
                &self.cfg,
                &self.hw,
                &self.readings,
                &self.css,
                &self.active,
                index,
                &self.command_cache,
                &roots.proc_root,
            ))
        };
        if let Some(html) = html
            && self.tooltip_html.as_ref() != Some(&html)
        {
            write_atomic(&paths.tooltip, &html)?;
            self.tooltip_html = Some(html);
        }
        Ok(())
    }

    fn pending_page(&self, index: usize) -> String {
        let page = self.active[index];
        let header = title_html(&page);
        let footer = pager_html(
            index,
            self.cfg.display.tooltip_width.max(0) as usize,
            self.active.len(),
        );
        PageFormatter::new(&self.cfg, &self.hw).format_page(
            "<div class=\"page\"></div>",
            &self.css,
            &header,
            &footer,
        )
    }

    pub(super) fn update_style(&mut self, paths: &DaemonPaths, force: bool) -> bool {
        let kde_stamp = mtime(&paths.kdeglobals);
        let mut changed = force;
        if force || kde_stamp != self.kde_stamp {
            self.kde_stamp = kde_stamp;
            self.theme_reconciliation_pending = true;
            let light = super::super::kdeglobals_background(&paths.kdeglobals)
                .is_some_and(super::super::is_light_rgb);
            let path = resolve_style(if light {
                "style-light.css"
            } else {
                "style-dark.css"
            });
            if path != self.css_path {
                self.css_path = path;
                changed = true;
            }
        }
        if let Some(name) = self.css_path.file_name() {
            let path = resolve_style(&name.to_string_lossy());
            if path != self.css_path {
                self.css_path = path;
                changed = true;
            }
        }
        let overlay = self
            .cfg
            .display
            .overlay
            .then(|| resolve_style("style-overlay.css"));
        changed |= overlay != self.overlay_path;
        self.overlay_path = overlay;
        self.reload_style_files(changed)
    }

    pub(super) fn reload_style_files(&mut self, force: bool) -> bool {
        let css_stamp = mtime(&self.css_path);
        let overlay_stamp = self.overlay_path.as_deref().and_then(mtime);
        let changed = force || css_stamp != self.css_stamp || overlay_stamp != self.overlay_stamp;
        if changed {
            self.css_stamp = css_stamp;
            self.overlay_stamp = overlay_stamp;
            self.css = read_css(&self.css_path, self.overlay_path.as_deref());
            self.rendered_graph = None;
            self.style_generation = self.style_generation.saturating_add(1);
            self.render_generation = self.render_generation.saturating_add(1);
        }
        changed
    }

    pub(super) fn apply_theme(&mut self, light: bool) -> bool {
        let path = resolve_style(if light {
            "style-light.css"
        } else {
            "style-dark.css"
        });
        if path == self.css_path {
            return false;
        }
        self.css_path = path;
        self.css_stamp = mtime(&self.css_path);
        self.css = read_css(&self.css_path, self.overlay_path.as_deref());
        self.rendered_graph = None;
        self.style_generation = self.style_generation.saturating_add(1);
        self.render_generation = self.render_generation.saturating_add(1);
        true
    }
}

fn apply_inventory_completion(
    completion: &JobCompletion,
    hw: &mut HardwareInventory,
    resolved_mounts: &mut Vec<String>,
) {
    match completion.ticket.job.kind {
        crate::scheduler::JobKind::NetworkIdentity => {
            hw.net_device.clone_from(&completion.hw.net_device);
        }
        crate::scheduler::JobKind::MountInventory => {
            resolved_mounts.clone_from(&completion.resolved_mounts);
        }
        crate::scheduler::JobKind::HardwareDiscovery => {
            let crate::scheduler::SourceIdentity::Inventory(family) = completion.ticket.job.source
            else {
                return;
            };
            apply_inventory_family(family, &completion.hw, hw);
        }
        _ => {}
    }
}

fn apply_inventory_family(
    family: InventoryFamily,
    source: &HardwareInventory,
    target: &mut HardwareInventory,
) {
    match family {
        InventoryFamily::Cpu => {
            target.cpu_temp_path.clone_from(&source.cpu_temp_path);
            target.cpu_freq_path.clone_from(&source.cpu_freq_path);
            target.cpu_turbo_path.clone_from(&source.cpu_turbo_path);
            target.cpu_turbo_supported = source.cpu_turbo_supported;
        }
        InventoryFamily::Thermal => {
            target.hd_temp_paths.clone_from(&source.hd_temp_paths);
            target.fan_paths.clone_from(&source.fan_paths);
        }
        InventoryFamily::SystemBattery => {
            target.battery_sys_ids.clone_from(&source.battery_sys_ids);
        }
        InventoryFamily::Smart => {
            target
                .disk_smart_drives
                .clone_from(&source.disk_smart_drives);
        }
        InventoryFamily::Nvidia => target.has_nvidia = source.has_nvidia,
        InventoryFamily::Intel => {
            target
                .intel_gpu_freq_path
                .clone_from(&source.intel_gpu_freq_path);
            target.intel_gpu_pci.clone_from(&source.intel_gpu_pci);
        }
        InventoryFamily::Backlight => target.has_backlight = source.has_backlight,
        InventoryFamily::Network => {
            target.net_device.clone_from(&source.net_device);
            target.has_wifi = source.has_wifi;
        }
        InventoryFamily::DiskIo => {
            target.disk_io_device.clone_from(&source.disk_io_device);
        }
        InventoryFamily::Peripheral => {
            target.battery_mouse_id.clone_from(&source.battery_mouse_id);
            target.battery_kbd_id.clone_from(&source.battery_kbd_id);
        }
    }
}

fn apply_owner_readings(owner: OwnerId, source: &DisplaySnapshot, target: &mut DisplaySnapshot) {
    match owner {
        OwnerId::Cpu => {
            target.cpu_usage = source.cpu_usage;
            target.cpu_temp = source.cpu_temp;
            target.cpu_freq_mhz = source.cpu_freq_mhz;
            target.cpu_turbo = source.cpu_turbo;
            target.cpu_history.clone_from(&source.cpu_history);
            target.uptime_seconds = source.uptime_seconds;
            target.load_average = source.load_average;
            target.cpu_core_usage.clone_from(&source.cpu_core_usage);
            target.cpu_core_history.clone_from(&source.cpu_core_history);
        }
        OwnerId::Process => {
            target.top_process.clone_from(&source.top_process);
            target.top_process_full.clone_from(&source.top_process_full);
        }
        OwnerId::Memory => {
            target.mem_history.clone_from(&source.mem_history);
            target.mem_usage = source.mem_usage;
            target.mem_used_gib = source.mem_used_gib;
            target.mem_total_gib = source.mem_total_gib;
            target.swap_usage = source.swap_usage;
        }
        OwnerId::Network => {
            target.net_up_bps = source.net_up_bps;
            target.net_down_bps = source.net_down_bps;
            target.net_device.clone_from(&source.net_device);
            target.ip_address.clone_from(&source.ip_address);
            target.wifi_ssid.clone_from(&source.wifi_ssid);
            target.wifi_signal_percent = source.wifi_signal_percent;
            target.net_up_history.clone_from(&source.net_up_history);
            target.net_down_history.clone_from(&source.net_down_history);
        }
        OwnerId::Disk => {
            target.disk_read_bps = source.disk_read_bps;
            target.disk_write_bps = source.disk_write_bps;
            target.disk_usage.clone_from(&source.disk_usage);
            target.disk_smart.clone_from(&source.disk_smart);
            target.hd_temps.clone_from(&source.hd_temps);
            target.fan_speeds.clone_from(&source.fan_speeds);
        }
        OwnerId::Power => {
            target.battery_sys.clone_from(&source.battery_sys);
            target.battery_mouse.clone_from(&source.battery_mouse);
            target.battery_kbd.clone_from(&source.battery_kbd);
        }
        OwnerId::Nvidia => {
            target.gpu_temp = source.gpu_temp;
            target.gpu_usage = source.gpu_usage;
            target.gpu_mem = source.gpu_mem;
            target.gpu_dec = source.gpu_dec;
            target.gpu_fan = source.gpu_fan;
        }
        OwnerId::IntelGpu => {
            target.gpu_intel_freq = source.gpu_intel_freq;
            target.gpu_intel_usage = source.gpu_intel_usage;
            target.gpu_intel_dec_usage = source.gpu_intel_dec_usage;
        }
        OwnerId::GpuHistory => {
            target
                .gpu_usage_history
                .clone_from(&source.gpu_usage_history);
            target.gpu_dec_history.clone_from(&source.gpu_dec_history);
        }
        OwnerId::External => {
            target.screen_brightness = source.screen_brightness;
            target.system_updates = source.system_updates;
            target.server_ok = source.server_ok;
        }
        OwnerId::Page | OwnerId::Discovery => {}
    }
}

pub(super) fn config_watch_paths(config_path: Option<&Path>) -> (PathBuf, Vec<PathBuf>) {
    (
        config_path.map_or_else(default_config_path, Path::to_path_buf),
        machine_source_paths(config_path),
    )
}

pub(super) fn load_replacement(config_path: Option<&Path>) -> Result<Config> {
    load_config(config_path, None).map_err(Into::into)
}

pub(super) fn build_replacement_pages(cfg: &Config) -> Vec<Page> {
    build_pages(&cfg.pages.order)
}
