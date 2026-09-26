//! Causal Human Liveness prior.
//!
//! This is a multiplier on the final ranking score, not a public badge and
//! not a rewrite of Phoenix's weighted sum. Missing evidence and wide
//! posteriors return 1.0. The reaction field may only contain actions that
//! have already arrived: `lagged_*` is P(E, [t-W, t)), never the future.

use serde::{Deserialize, Serialize};

/// Private features a hydrator may attach to a candidate.
/// None of these fields are user-visible.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PulseEvidence {
    /// Posterior mean of independently-acting-human mass, in [0, 1].
    pub liveness: f64,
    /// Posterior standard deviation. Wide means "do nothing".
    pub sigma: f64,
    /// Residual independence after conditioning on the stimulus and feed.
    pub independence: f64,
    /// Predictive lift of a cluster residual after the stimulus shown.
    /// High is interesting. It is not guilt.
    pub ci: f64,
    /// Count of reactions already observed in the lagged window.
    pub lagged_n: u32,
    /// Share of those reactions attributed to a coordinated cluster.
    pub lagged_swarm_share: f64,
    /// Shannon diversity of action kinds in the lagged window, in [0, 1].
    pub lagged_diversity: f64,
    /// Synchronization of lagged latencies, in [0, 1].
    pub lagged_sync: f64,
}

impl Default for PulseEvidence {
    fn default() -> Self {
        Self {
            liveness: 0.5,
            sigma: 0.28,
            independence: 0.5,
            ci: 0.2,
            lagged_n: 0,
            lagged_swarm_share: 0.0,
            lagged_diversity: 0.0,
            lagged_sync: 0.0,
        }
    }
}

fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// Below this width the posterior is too uncertain to touch ranking.
const ACT_SIGMA: f64 = 0.18;

/// 1.0 unless the flag is on and the posterior is narrow enough to act.
pub fn causal_multiplier(enabled: bool, evidence: Option<&PulseEvidence>) -> f64 {
    if !enabled {
        return 1.0;
    }
    let Some(evidence) = evidence else {
        return 1.0;
    };
    if !evidence.sigma.is_finite() || evidence.sigma > ACT_SIGMA {
        return 1.0;
    }
    causal_q(evidence)
}

/// Streaming q_t. Early posts (lagged_n < 4) use only author-side state.
pub fn causal_q(evidence: &PulseEvidence) -> f64 {
    let liveness = clamp01(evidence.liveness);
    let independence = clamp01(evidence.independence);
    let ci = clamp01(evidence.ci);
    let confidence = clamp01(1.0 - evidence.sigma / 0.3);
    let author = 0.55 * liveness + 0.3 * independence + 0.15 * (1.0 - ci);
    if evidence.lagged_n < 4 {
        return 0.42 + 0.58 * (author * (0.55 + 0.45 * confidence));
    }
    let event = 0.45 * (1.0 - clamp01(evidence.lagged_swarm_share))
        + 0.3 * clamp01(evidence.lagged_diversity)
        + 0.25 * (1.0 - clamp01(evidence.lagged_sync));
    0.28 + 0.72 * (0.55 * author * (0.4 + 0.6 * confidence) + 0.45 * event)
}

/// Derived liveness from a class posterior. Not a binary expectation.
/// Classes: human, mixed, agent, service, coordinated.
pub fn liveness_from_posterior(p: [f64; 5]) -> f64 {
    let sum = p.iter().sum::<f64>();
    if sum <= 0.0 {
        return 0.5;
    }
    let w = [1.0, 0.72, 0.42, 0.22, 0.06];
    p.iter().zip(w).map(|(pi, wi)| (pi / sum) * wi).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_or_missing_or_wide_is_identity() {
        let swarm = PulseEvidence {
            liveness: 0.1,
            sigma: 0.04,
            independence: 0.1,
            ci: 0.9,
            lagged_n: 30,
            lagged_swarm_share: 0.95,
            lagged_diversity: 0.1,
            lagged_sync: 0.9,
        };
        assert_eq!(causal_multiplier(false, Some(&swarm)), 1.0);
        assert_eq!(causal_multiplier(true, None), 1.0);
        let wide = PulseEvidence { sigma: 0.26, ..swarm };
        assert_eq!(causal_multiplier(true, Some(&wide)), 1.0);
    }

    #[test]
    fn early_window_ignores_event_field() {
        let author = PulseEvidence {
            liveness: 0.8,
            sigma: 0.05,
            independence: 0.75,
            ci: 0.15,
            lagged_n: 1,
            lagged_swarm_share: 1.0,
            lagged_diversity: 0.0,
            lagged_sync: 1.0,
        };
        let q = causal_q(&author);
        assert!(q > 0.7, "author-only q should stay high, got {q}");
    }

    #[test]
    fn coordinated_lagged_field_discounts() {
        let human = PulseEvidence {
            liveness: 0.78,
            sigma: 0.06,
            independence: 0.74,
            ci: 0.16,
            lagged_n: 20,
            lagged_swarm_share: 0.05,
            lagged_diversity: 0.8,
            lagged_sync: 0.2,
        };
        let swarm = PulseEvidence {
            liveness: 0.18,
            sigma: 0.05,
            independence: 0.12,
            ci: 0.84,
            lagged_n: 20,
            lagged_swarm_share: 0.92,
            lagged_diversity: 0.15,
            lagged_sync: 0.9,
        };
        let qh = causal_q(&human);
        let qs = causal_q(&swarm);
        assert!(qh > qs, "human {qh} should outrank swarm {qs}");
        assert!(qs < 0.55, "swarm q should discount, got {qs}");
    }

    #[test]
    fn liveness_is_a_mixture_not_a_binary() {
        let pure_human = liveness_from_posterior([1.0, 0.0, 0.0, 0.0, 0.0]);
        let mixed = liveness_from_posterior([0.0, 1.0, 0.0, 0.0, 0.0]);
        let coordinated = liveness_from_posterior([0.0, 0.0, 0.0, 0.0, 1.0]);
        assert!((pure_human - 1.0).abs() < 1e-9);
        assert!(mixed < pure_human && mixed > coordinated);
        assert!(coordinated < 0.1);
    }
}
