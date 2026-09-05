//! Selected GPU graph-history ownership.

use std::time::Duration;

use crate::config::Config;
use crate::domain::boundary::ClockSnapshot;
use crate::domain::readings::{DisplaySnapshot, HardwareInventory, MetricSample};

/// Selected GPU graph histories, independent of the vendor owners.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GpuHistoryState {
    /// Selected GPU owner identity for the retained history.
    pub source: Option<String>,
    /// Active GPU usage history.
    pub usage: Vec<i32>,
    /// Active GPU decoder or AMD codec history.
    pub decoder: Vec<i32>,
    /// Latest real decoder or AMD codec sample, retained independently from usage.
    pub latest_decoder: Option<i32>,
    /// Monotonic instant of the last history sample.
    pub sampled_at: Option<Duration>,
    /// Latest completed usage-history sample.
    pub usage_sample: Option<MetricSample<Vec<i32>>>,
    /// Latest completed decoder-history sample.
    pub decoder_sample: Option<MetricSample<Vec<i32>>>,
}

impl GpuHistoryState {
    #[cfg(test)]
    pub(crate) fn is_due(&self, hw: &HardwareInventory, now: Duration, cadence: Duration) -> bool {
        self.source != selected_source(hw)
            || self
                .sampled_at
                .is_none_or(|sampled_at| now.saturating_sub(sampled_at) >= cadence)
    }

    /// Reconciles the selected GPU source and optionally appends one history point.
    #[must_use]
    pub fn sample(
        &mut self,
        cfg: &Config,
        hw: &HardwareInventory,
        readings: &DisplaySnapshot,
        decoder_outcome: DecoderOutcome,
        clock: ClockSnapshot,
        append: bool,
    ) -> Option<GpuHistoryResult> {
        if !cfg.pages.order.iter().any(|page| page == "graphs") {
            *self = Self::default();
            return None;
        }
        let Some(source) = selected_source(hw) else {
            *self = Self::default();
            return None;
        };
        let usage = if hw.has_nvidia {
            readings.gpu_usage
        } else if hw.amd_gpu.is_some() {
            readings.gpu_amd_usage
        } else {
            readings.gpu_intel_usage
        };
        if self.source.as_ref() != Some(&source) {
            *self = Self {
                source: Some(source),
                ..Self::default()
            };
        }

        match decoder_outcome {
            DecoderOutcome::Value(value) => self.latest_decoder = Some(value),
            DecoderOutcome::ConfirmedAbsent => self.latest_decoder = None,
            DecoderOutcome::TransientFailure | DecoderOutcome::Unmeasured => {
                // AMD supplies retained, deadline-eligible codec values as Value; no value must not revive an invalidated source.
                if !hw.has_nvidia && hw.amd_gpu.is_some() {
                    self.latest_decoder = None;
                }
            }
        }

        if append {
            self.sampled_at = Some(clock.monotonic);
            let max_len = cfg.pages.graph_history_length.max(0) as usize;
            if let Some(usage) = usage {
                self.usage.push(usage);
                trim_to_len(&mut self.usage, max_len);
                self.usage_sample = Some(MetricSample::new(self.usage.clone(), clock.monotonic));
            }
            if let Some(decoder) = self.latest_decoder {
                self.decoder.push(decoder);
                trim_to_len(&mut self.decoder, max_len);
                self.decoder_sample =
                    Some(MetricSample::new(self.decoder.clone(), clock.monotonic));
            }
        }

        Some(GpuHistoryResult {
            usage: self.usage_sample.clone(),
            decoder: self.decoder_sample.clone(),
        })
    }
}

/// One selected GPU history result ready for display flattening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuHistoryResult {
    /// Latest completed usage-history sample.
    pub usage: Option<MetricSample<Vec<i32>>>,
    /// Latest completed decoder-history sample.
    pub decoder: Option<MetricSample<Vec<i32>>>,
}

/// Typed decoder outcome supplied by the selected GPU owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderOutcome {
    /// A valid current or retained decoder value is available.
    Value(i32),
    /// The latest attempt failed transiently, so the last value remains usable.
    TransientFailure,
    /// The source confirmed that decoder utilization is absent or unsupported.
    ConfirmedAbsent,
    /// No decoder attempt has produced a value or confirmed absence yet.
    Unmeasured,
}

/// Samples the preferred GPU into graphs-page history.
///
/// NVIDIA, AMDGPU, then Intel win by hardware presence, even when the preferred device has no current reading. A missing usage sample preserves and re-exposes existing history without inserting a gap.
#[must_use]
pub fn sample_gpu_history(
    state: &mut GpuHistoryState,
    cfg: &Config,
    hw: &HardwareInventory,
    readings: &DisplaySnapshot,
    decoder_outcome: DecoderOutcome,
    clock: ClockSnapshot,
    append: bool,
) -> Option<GpuHistoryResult> {
    state.sample(cfg, hw, readings, decoder_outcome, clock, append)
}

pub(crate) fn selected_source(hw: &HardwareInventory) -> Option<String> {
    if hw.has_nvidia {
        Some(String::from("nvidia"))
    } else if let Some(amd) = &hw.amd_gpu {
        Some(format!("amd:{}", amd.pci_identity))
    } else {
        hw.intel_gpu_pci
            .as_deref()
            .map(|pci| format!("intel:{pci}"))
    }
}

fn trim_to_len<T>(values: &mut Vec<T>, max_len: usize) {
    if values.len() > max_len {
        values.drain(..values.len() - max_len);
    }
}

#[cfg(test)]
mod tests;
