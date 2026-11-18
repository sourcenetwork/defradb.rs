//! Per-provider time-decayed estimates: how long a useful fetch takes, how
//! often a fetch is useful, and what any fetch costs. Three packed words.

use std::time::Duration;

use super::{
    PackedEwma, LATENCY_UNIT_MICROS, PROVIDER_HALF_LIFE, SCALE_PPM, USEFUL_FLOOR_PPM, VALUE_MAX,
};

const HALF_LIFE_MS: u64 = PROVIDER_HALF_LIFE.as_millis() as u64;

/// One provider's estimates. Any transport that measures its fetches can keep
/// one per peer; iroh's `FetchStats` does.
#[derive(Debug, Default)]
pub struct ProviderEstimates {
    /// Duration of a fetch that delivered blocks. A timeout is censored at its
    /// deadline, so it never feeds this one.
    success_latency: PackedEwma,
    /// `SCALE_PPM` for a fetch that delivered blocks, zero for any other.
    useful: PackedEwma,
    /// Duration of every finished fetch, whatever its outcome.
    attempt_cost: PackedEwma,
}

/// The estimates of one provider, as the sync status exports them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct ProviderEstimateExport {
    pub success_latency_micros: Option<u64>,
    pub useful_ewma_ppm: Option<u64>,
    pub attempt_cost_micros: Option<u64>,
    /// `attempt_cost / max(useful, 1/64)`: the expected time to a useful
    /// fetch from this provider.
    pub expected_useful_micros: Option<u64>,
    /// Time since the last finished fetch fed the estimates.
    pub ewma_age_millis: Option<u64>,
}

impl ProviderEstimates {
    /// A fetch finished after `duration`; `useful` when it delivered blocks.
    pub fn record(&self, now_ms: u64, duration: Duration, useful: bool) {
        let units = units(duration);
        if useful {
            self.success_latency.record(now_ms, units, HALF_LIFE_MS);
        }
        let share = if useful { SCALE_PPM } else { 0 };
        self.useful.record(now_ms, share, HALF_LIFE_MS);
        self.attempt_cost.record(now_ms, units, HALF_LIFE_MS);
    }

    pub fn export(&self, now_ms: u64) -> ProviderEstimateExport {
        let micros = |units: u64| units.saturating_mul(LATENCY_UNIT_MICROS);
        let cost = self.attempt_cost.read();
        let useful = self.useful.read().map(|useful| useful.value);
        ProviderEstimateExport {
            success_latency_micros: self.success_latency.read().map(|e| micros(e.value)),
            useful_ewma_ppm: useful,
            attempt_cost_micros: cost.map(|e| micros(e.value)),
            expected_useful_micros: cost
                .zip(useful)
                .map(|(cost, useful)| expected_useful(micros(cost.value), useful)),
            ewma_age_millis: cost.map(|e| now_ms.saturating_sub(e.at)),
        }
    }
}

/// `cost / max(useful, USEFUL_FLOOR_PPM)` in the cost's unit: at most 64 times
/// the cost, and the cost alone for a provider that is always useful.
pub(crate) fn expected_useful(cost: u64, useful_ppm: u64) -> u64 {
    let useful = useful_ppm.max(USEFUL_FLOOR_PPM);
    u64::try_from(u128::from(cost) * u128::from(SCALE_PPM) / u128::from(useful)).unwrap_or(u64::MAX)
}

fn units(duration: Duration) -> u64 {
    let units = duration.as_micros() / LATENCY_UNIT_MICROS as u128;
    u64::try_from(units).unwrap_or(VALUE_MAX).min(VALUE_MAX)
}
