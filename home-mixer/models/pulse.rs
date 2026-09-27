//! Causal Human Liveness prior.
//!
//! Multiplier on the final ranking score. Missing evidence and wide
//! posteriors return 1.0. The reaction field is P(E, [t-W, t)), never the future.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PulseEvidence {
    pub liveness: f64,
    pub sigma: f64,
    pub independence: f64,
    pub ci: f64,
    pub lagged_n: u32,
    pub lagged_swarm_share: f64,
    pub lagged_diversity: f64,
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

const ACT_SIGMA: f64 = 0.18;
pub const LAGGED_WINDOW_MS: u64 = 30 * 60 * 1000;
const MIN_CI_ROWS: usize = 16;
const UNLABELED_SIGMA_FLOOR: f64 = 0.22;

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

pub fn liveness_from_posterior(p: [f64; 5]) -> f64 {
    let sum = p.iter().sum::<f64>();
    if sum <= 0.0 {
        return 0.5;
    }
    let w = [1.0, 0.72, 0.42, 0.22, 0.06];
    p.iter().zip(w).map(|(pi, wi)| (pi / sum) * wi).sum()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionKind {
    Favorite,
    Reply,
    Repost,
    Quote,
    Click,
    Other,
}

impl ReactionKind {
    const COUNT: usize = 6;
    fn index(self) -> usize {
        match self {
            Self::Favorite => 0,
            Self::Reply => 1,
            Self::Repost => 2,
            Self::Quote => 3,
            Self::Click => 4,
            Self::Other => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclaredClass {
    Human,
    Agent,
    Service,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionEvent {
    pub actor_id: u64,
    pub at_ms: u64,
    pub kind: ReactionKind,
    pub cluster_id: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecisionRow {
    pub at_ms: u64,
    pub reacted: bool,
    pub stimulus: f64,
    pub own_rate: f64,
    pub feed_rate: f64,
    pub cluster_rate: f64,
}

#[derive(Clone, Debug, Default)]
pub struct AuthorObservations {
    pub declared: Option<DeclaredClass>,
    pub decisions: Vec<DecisionRow>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaggedField {
    pub n: u32,
    pub swarm_share: f64,
    pub diversity: f64,
    pub sync: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RankEffect {
    pub multiplier: f64,
    pub moves: bool,
}

fn in_reaction_window(at_ms: u64, scoring_time_ms: u64, window_ms: u64) -> bool {
    at_ms < scoring_time_ms && at_ms >= scoring_time_ms.saturating_sub(window_ms)
}

pub fn lagged_field(
    scoring_time_ms: u64,
    window_ms: u64,
    reactions: &[ReactionEvent],
) -> LaggedField {
    let kept: Vec<ReactionEvent> = reactions
        .iter()
        .copied()
        .filter(|event| in_reaction_window(event.at_ms, scoring_time_ms, window_ms))
        .collect();
    if kept.is_empty() {
        return LaggedField {
            n: 0,
            swarm_share: 0.0,
            diversity: 0.0,
            sync: 0.0,
        };
    }
    let mut kind_counts = [0u32; ReactionKind::COUNT];
    let mut cluster_counts: Vec<(u32, u32)> = Vec::new();
    for event in &kept {
        kind_counts[event.kind.index()] += 1;
        if let Some(cluster_id) = event.cluster_id {
            if let Some(slot) = cluster_counts.iter_mut().find(|(id, _)| *id == cluster_id) {
                slot.1 += 1;
            } else {
                cluster_counts.push((cluster_id, 1));
            }
        }
    }
    let largest = cluster_counts.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let n = kept.len() as u32;
    LaggedField {
        n,
        swarm_share: largest as f64 / f64::from(n),
        diversity: 0.0,
        sync: 0.0,
    }
}

/// Time-split residual lift placeholder. Same name as the Sept 1 slice.
/// Full OLS lives on `pulse/causal-q`. This copy keeps the gate: high CI
/// does not narrow sigma for unlabeled accounts.
pub fn counterfactual_independence(rows: &[DecisionRow]) -> f64 {
    if rows.len() < MIN_CI_ROWS {
        return 0.0;
    }
    let mean_cluster =
        rows.iter().map(|r| r.cluster_rate).sum::<f64>() / rows.len() as f64;
    clamp01(mean_cluster)
}

pub fn class_posterior(declared: Option<DeclaredClass>, ci: f64, n_decisions: usize) -> [f64; 5] {
    let ci = clamp01(ci);
    match declared {
        Some(DeclaredClass::Human) => [0.86, 0.10, 0.02, 0.01, 0.01],
        Some(DeclaredClass::Agent) => [0.05, 0.15, 0.70, 0.05, 0.05],
        Some(DeclaredClass::Service) => [0.04, 0.08, 0.08, 0.74, 0.06],
        None if n_decisions < MIN_CI_ROWS => [0.2, 0.2, 0.2, 0.2, 0.2],
        None => {
            let human = 0.55 * (1.0 - ci) + 0.05;
            let coordinated = 0.50 * ci + 0.05;
            let rest = ((1.0 - human - coordinated) / 3.0).max(0.0);
            [human, rest, rest, rest, coordinated]
        }
    }
}

pub fn posterior_sigma(
    posterior: &[f64; 5],
    n_decisions: u32,
    declared: Option<DeclaredClass>,
) -> f64 {
    match declared {
        Some(DeclaredClass::Agent) | Some(DeclaredClass::Service) => 0.08,
        Some(DeclaredClass::Human) | None => {
            if n_decisions < MIN_CI_ROWS as u32 {
                return 0.30;
            }
            UNLABELED_SIGMA_FLOOR
        }
    }
}

pub fn evidence_from_observations(
    scoring_time_ms: u64,
    window_ms: u64,
    reactions: &[ReactionEvent],
    author: Option<&AuthorObservations>,
) -> Option<PulseEvidence> {
    let field = lagged_field(scoring_time_ms, window_ms, reactions);
    let past: Vec<DecisionRow> = author
        .map(|observations| {
            observations
                .decisions
                .iter()
                .copied()
                .filter(|row| row.at_ms < scoring_time_ms)
                .collect()
        })
        .unwrap_or_default();
    let declared = author.and_then(|observations| observations.declared);
    if field.n == 0 && declared.is_none() && past.is_empty() {
        return None;
    }
    let ci = counterfactual_independence(&past);
    let posterior = class_posterior(declared, ci, past.len());
    let sigma = posterior_sigma(&posterior, past.len() as u32, declared);
    Some(PulseEvidence {
        liveness: liveness_from_posterior(posterior),
        sigma,
        independence: if past.len() < MIN_CI_ROWS {
            0.5
        } else {
            clamp01(1.0 - ci)
        },
        ci,
        lagged_n: field.n,
        lagged_swarm_share: field.swarm_share,
        lagged_diversity: field.diversity,
        lagged_sync: field.sync,
    })
}

pub fn rank_effect(enabled: bool, evidence: &PulseEvidence) -> RankEffect {
    let multiplier = causal_multiplier(enabled, Some(evidence));
    RankEffect {
        multiplier,
        moves: (multiplier - 1.0).abs() > 1e-12,
    }
}

/// `P_u(E, t) = P(E, t) \ B_u`
pub fn viewer_influence_mask(
    events: &[ReactionEvent],
    excluded_user_ids: &[i64],
) -> Vec<ReactionEvent> {
    if excluded_user_ids.is_empty() {
        return events.to_vec();
    }
    events
        .iter()
        .copied()
        .filter(|event| {
            let actor = i64::try_from(event.actor_id).unwrap_or(i64::MAX);
            !excluded_user_ids.contains(&actor)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_or_missing_or_wide_is_identity() {
        assert_eq!(causal_multiplier(false, Some(&PulseEvidence { sigma: 0.04, ..Default::default() })), 1.0);
        assert_eq!(causal_multiplier(true, None), 1.0);
        assert_eq!(causal_multiplier(true, Some(&PulseEvidence { sigma: 0.26, ..Default::default() })), 1.0);
    }

    #[test]
    fn unlabeled_stays_wide_declared_agent_moves() {
        let unlabeled = evidence_from_observations(
            10_000,
            LAGGED_WINDOW_MS,
            &[ReactionEvent { actor_id: 1, at_ms: 9_000, kind: ReactionKind::Favorite, cluster_id: Some(1) }],
            None,
        )
        .unwrap();
        assert!(unlabeled.sigma > 0.18);
        assert!(!rank_effect(true, &unlabeled).moves);

        let agent = evidence_from_observations(
            10_000,
            LAGGED_WINDOW_MS,
            &[],
            Some(&AuthorObservations { declared: Some(DeclaredClass::Agent), decisions: Vec::new() }),
        )
        .unwrap();
        assert!((agent.sigma - 0.08).abs() < 1e-12);
        assert!(rank_effect(true, &agent).moves);
    }

    #[test]
    fn viewer_mask_drops_blocked_actors_and_keeps_others() {
        let events = [
            ReactionEvent { actor_id: 10, at_ms: 1, kind: ReactionKind::Favorite, cluster_id: None },
            ReactionEvent { actor_id: 20, at_ms: 2, kind: ReactionKind::Repost, cluster_id: Some(1) },
            ReactionEvent { actor_id: 30, at_ms: 3, kind: ReactionKind::Reply, cluster_id: None },
        ];
        let kept = viewer_influence_mask(&events, &[20, 99]);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].actor_id, 10);
        assert_eq!(kept[1].actor_id, 30);
    }
}
