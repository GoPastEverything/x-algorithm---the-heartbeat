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

/// Reactions already on the clock. `at_ms >= scoring_time` is not a reaction yet.
pub const LAGGED_WINDOW_MS: u64 = 30 * 60 * 1000;

const MIN_CI_ROWS: usize = 16;
/// Unlabeled and declared-human posteriors stay wider than [`ACT_SIGMA`].
const UNLABELED_SIGMA_FLOOR: f64 = 0.22;

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

/// Account label supplied by the caller. This module does not infer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclaredClass {
    Human,
    Agent,
    Service,
}

/// One reaction with an actor and a time. Not an engagement count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionEvent {
    pub actor_id: u64,
    pub at_ms: u64,
    pub kind: ReactionKind,
    /// Upstream cluster label. `None` is "not assigned", not "a human".
    pub cluster_id: Option<u32>,
}

/// One past decision by the author being ranked.
/// `cluster_rate` must already exclude this row.
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
    if at_ms >= scoring_time_ms {
        return false;
    }
    at_ms >= scoring_time_ms.saturating_sub(window_ms)
}

/// P(E, [t−W, t)). Events at `scoring_time_ms` or later are dropped.
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
        diversity: shannon_diversity(&kind_counts),
        sync: latency_sync(&kept, window_ms),
    }
}

fn shannon_diversity(counts: &[u32]) -> f64 {
    let n: u32 = counts.iter().sum();
    if n == 0 || counts.len() < 2 {
        return 0.0;
    }
    let mut entropy = 0.0;
    for &count in counts {
        if count == 0 {
            continue;
        }
        let p = f64::from(count) / f64::from(n);
        entropy -= p * p.ln();
    }
    clamp01(entropy / (counts.len() as f64).ln())
}

fn latency_sync(events: &[ReactionEvent], window_ms: u64) -> f64 {
    if events.len() < 2 || window_ms == 0 {
        return 0.0;
    }
    let mean = events.iter().map(|event| event.at_ms as f64).sum::<f64>() / events.len() as f64;
    let variance = events
        .iter()
        .map(|event| {
            let delta = event.at_ms as f64 - mean;
            delta * delta
        })
        .sum::<f64>()
        / events.len() as f64;
    clamp01(1.0 - variance.sqrt() / (window_ms as f64 / 2.0))
}

/// Time-split predictive lift of a lagged cluster residual.
/// High is a shared controller. It is not a bot verdict, and it does not
/// by itself narrow `sigma` under the action threshold.
pub fn counterfactual_independence(rows: &[DecisionRow]) -> f64 {
    if rows.len() < MIN_CI_ROWS {
        return 0.0;
    }
    let cut = rows.len() / 2;
    let train = &rows[..cut];
    let test = &rows[cut..];
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    for row in train {
        low = low.min(row.cluster_rate);
        high = high.max(row.cluster_rate);
    }
    if !low.is_finite() || high - low < 1e-6 {
        return 0.0;
    }
    let (train_base, y_train) = design(train, false);
    let (train_full, _) = design(train, true);
    let Some(base_weights) = ols(&train_base, &y_train) else {
        return 0.0;
    };
    let Some(full_weights) = ols(&train_full, &y_train) else {
        return 0.0;
    };
    let (test_base, y_test) = design(test, false);
    let (test_full, _) = design(test, true);
    let lift = mean_bernoulli_ll(&full_weights, &test_full, &y_test)
        - mean_bernoulli_ll(&base_weights, &test_base, &y_test);
    if lift <= 0.0 {
        0.0
    } else {
        1.0 - (-lift / 0.35).exp()
    }
}

fn design(rows: &[DecisionRow], with_cluster: bool) -> (Vec<Vec<f64>>, Vec<f64>) {
    let features = rows
        .iter()
        .map(|row| {
            let mut features = vec![1.0, row.stimulus, row.own_rate, row.feed_rate];
            if with_cluster {
                features.push(row.cluster_rate);
            }
            features
        })
        .collect();
    let outcomes = rows
        .iter()
        .map(|row| if row.reacted { 1.0 } else { 0.0 })
        .collect();
    (features, outcomes)
}

fn ols(rows: &[Vec<f64>], outcomes: &[f64]) -> Option<Vec<f64>> {
    if rows.is_empty() || rows.len() != outcomes.len() {
        return None;
    }
    let width = rows[0].len();
    let mut normal = vec![vec![0.0; width]; width];
    let mut target = vec![0.0; width];
    for (features, &outcome) in rows.iter().zip(outcomes) {
        if features.len() != width {
            return None;
        }
        for j in 0..width {
            target[j] += features[j] * outcome;
            for m in 0..width {
                normal[j][m] += features[j] * features[m];
            }
        }
    }
    for j in 0..width {
        normal[j][j] += 1e-3;
    }
    solve_linear(normal, target)
}

fn solve_linear(mut matrix: Vec<Vec<f64>>, mut target: Vec<f64>) -> Option<Vec<f64>> {
    let width = target.len();
    for col in 0..width {
        let mut pivot = col;
        for row in (col + 1)..width {
            if matrix[row][col].abs() > matrix[pivot][col].abs() {
                pivot = row;
            }
        }
        matrix.swap(col, pivot);
        target.swap(col, pivot);
        let divisor = matrix[col][col];
        if divisor.abs() < 1e-12 {
            return None;
        }
        for j in col..width {
            matrix[col][j] /= divisor;
        }
        target[col] /= divisor;
        for row in 0..width {
            if row == col {
                continue;
            }
            let factor = matrix[row][col];
            for j in col..width {
                matrix[row][j] -= factor * matrix[col][j];
            }
            target[row] -= factor * target[col];
        }
    }
    Some(target)
}

fn mean_bernoulli_ll(weights: &[f64], rows: &[Vec<f64>], outcomes: &[f64]) -> f64 {
    let mut total = 0.0;
    for (features, &outcome) in rows.iter().zip(outcomes) {
        let score: f64 = weights.iter().zip(features).map(|(w, x)| w * x).sum();
        let p = score.clamp(0.01, 0.99);
        total += outcome * p.ln() + (1.0 - outcome) * (1.0 - p).ln();
    }
    total / outcomes.len() as f64
}

/// Classes: human, mixed, agent, service, coordinated.
/// A missing label is uniform. CI may shift mass; it does not spike a class.
pub fn class_posterior(declared: Option<DeclaredClass>, ci: f64, n_decisions: usize) -> [f64; 5] {
    let ci = clamp01(ci);
    let raw = match declared {
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
    };
    normalize(raw)
}

fn normalize(mut p: [f64; 5]) -> [f64; 5] {
    for value in &mut p {
        if !value.is_finite() || *value < 0.0 {
            *value = 0.0;
        }
    }
    let sum = p.iter().sum::<f64>();
    if sum <= 0.0 {
        return [0.2; 5];
    }
    for value in &mut p {
        *value /= sum;
    }
    p
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
            let entropy = normalized_entropy(posterior);
            let data = 1.0 - (-(f64::from(n_decisions) / 48.0)).exp();
            let raw = 0.05 + 0.28 * entropy + 0.12 * (1.0 - data);
            raw.max(UNLABELED_SIGMA_FLOOR)
        }
    }
}

fn normalized_entropy(posterior: &[f64; 5]) -> f64 {
    let mut entropy = 0.0;
    for &p in posterior {
        if p > 0.0 {
            entropy -= p * p.ln();
        }
    }
    clamp01(entropy / 5f64.ln())
}

/// `None` when there is no past reaction and no author observation.
/// A reaction field alone keeps `sigma` wide, so ranking does not move.
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
    let independence = if past.len() < MIN_CI_ROWS {
        0.5
    } else {
        clamp01(1.0 - ci)
    };
    Some(PulseEvidence {
        liveness: liveness_from_posterior(posterior),
        sigma,
        independence,
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
        let wide = PulseEvidence {
            sigma: 0.26,
            ..swarm
        };
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

    fn event(at_ms: u64, kind: ReactionKind, cluster_id: Option<u32>) -> ReactionEvent {
        ReactionEvent {
            actor_id: at_ms,
            at_ms,
            kind,
            cluster_id,
        }
    }

    #[test]
    fn future_and_stale_reactions_are_not_in_the_window() {
        let scoring = 1_000_000u64;
        let window = 1_000u64;
        let field = lagged_field(
            scoring,
            window,
            &[
                event(scoring, ReactionKind::Favorite, None),
                event(scoring + 5, ReactionKind::Favorite, None),
                event(scoring - window - 1, ReactionKind::Reply, None),
                event(scoring - 1, ReactionKind::Quote, Some(7)),
            ],
        );
        assert_eq!(field.n, 1);
        assert!((field.swarm_share - 1.0).abs() < 1e-9);
    }

    #[test]
    fn unlabeled_reactions_are_not_a_cluster() {
        let scoring = 50_000u64;
        let field = lagged_field(
            scoring,
            10_000,
            &[
                event(40_000, ReactionKind::Favorite, None),
                event(40_100, ReactionKind::Reply, Some(3)),
                event(40_200, ReactionKind::Repost, Some(3)),
                event(40_300, ReactionKind::Click, Some(9)),
            ],
        );
        assert!((field.swarm_share - 0.5).abs() < 1e-9);
    }

    #[test]
    fn swarm_burst_is_tighter_and_less_diverse_than_a_human_trickle() {
        let scoring = 20_000u64;
        let window = 10_000u64;
        let human: Vec<ReactionEvent> = (0..12)
            .map(|i| {
                let kind = match i % 6 {
                    0 => ReactionKind::Favorite,
                    1 => ReactionKind::Reply,
                    2 => ReactionKind::Repost,
                    3 => ReactionKind::Quote,
                    4 => ReactionKind::Click,
                    _ => ReactionKind::Other,
                };
                event(10_000 + i * 800, kind, None)
            })
            .collect();
        let swarm: Vec<ReactionEvent> = (0..12)
            .map(|i| event(15_000 + (i % 3) * 20, ReactionKind::Favorite, Some(1)))
            .collect();
        let human_field = lagged_field(scoring, window, &human);
        let swarm_field = lagged_field(scoring, window, &swarm);
        assert!(swarm_field.sync > human_field.sync);
        assert!(human_field.diversity > swarm_field.diversity);
        assert!(swarm_field.swarm_share > 0.9);
        assert_eq!(human_field.swarm_share, 0.0);
    }

    fn fixed_decisions(cluster_drives: bool) -> Vec<DecisionRow> {
        (0..32)
            .map(|i| {
                let phase = (i % 8) as f64 / 7.0;
                let stimulus = if cluster_drives { 0.25 } else { phase };
                let cluster_rate = if cluster_drives {
                    phase
                } else {
                    ((i * 3) % 8) as f64 / 7.0
                };
                DecisionRow {
                    at_ms: i as u64 + 1,
                    reacted: if cluster_drives {
                        cluster_rate >= 0.5
                    } else {
                        stimulus >= 0.5
                    },
                    stimulus,
                    own_rate: if cluster_drives { 0.25 } else { 0.4 },
                    feed_rate: if cluster_drives { 0.25 } else { 0.4 },
                    cluster_rate,
                }
            })
            .collect()
    }

    #[test]
    fn counterfactual_independence_is_lift_not_a_label() {
        let swarm = counterfactual_independence(&fixed_decisions(true));
        let independent = counterfactual_independence(&fixed_decisions(false));
        assert!(swarm > 0.6, "cluster residual should lift, got {swarm}");
        assert!(
            independent < 0.05,
            "stimulus-only rows should not, got {independent}"
        );
    }

    #[test]
    fn reaction_field_alone_does_not_move_rank() {
        let scoring = 20_000u64;
        let swarm: Vec<ReactionEvent> = (0..12)
            .map(|i| event(15_000 + i, ReactionKind::Favorite, Some(4)))
            .collect();
        let evidence = evidence_from_observations(scoring, 10_000, &swarm, None).unwrap();
        assert!(evidence.lagged_n >= 4);
        assert!(evidence.sigma > 0.18, "sigma {}", evidence.sigma);
        let effect = rank_effect(true, &evidence);
        assert!(!effect.moves, "multiplier {}", effect.multiplier);
        assert!((effect.multiplier - 1.0).abs() < 1e-12);
    }

    #[test]
    fn high_unlabeled_ci_stays_above_the_action_threshold() {
        let author = AuthorObservations {
            declared: None,
            decisions: fixed_decisions(true),
        };
        let evidence =
            evidence_from_observations(10_000, LAGGED_WINDOW_MS, &[], Some(&author)).unwrap();
        assert!(evidence.ci > 0.6, "ci {}", evidence.ci);
        assert!(evidence.sigma >= 0.22, "sigma {}", evidence.sigma);
        assert!(!rank_effect(true, &evidence).moves);
    }

    #[test]
    fn declared_human_is_not_a_penalty_and_declared_agent_can_move() {
        let human = evidence_from_observations(
            10_000,
            LAGGED_WINDOW_MS,
            &[],
            Some(&AuthorObservations {
                declared: Some(DeclaredClass::Human),
                decisions: Vec::new(),
            }),
        )
        .unwrap();
        assert!(human.sigma > 0.18);
        assert!(!rank_effect(true, &human).moves);

        let agent = evidence_from_observations(
            10_000,
            LAGGED_WINDOW_MS,
            &[],
            Some(&AuthorObservations {
                declared: Some(DeclaredClass::Agent),
                decisions: Vec::new(),
            }),
        )
        .unwrap();
        assert!((agent.sigma - 0.08).abs() < 1e-12);
        assert!(
            agent.liveness > 0.40 && agent.liveness < 0.55,
            "liveness {}",
            agent.liveness
        );
        let effect = rank_effect(true, &agent);
        assert!(effect.moves);
        assert!(
            effect.multiplier < 0.85 && effect.multiplier > 0.55,
            "q {}",
            effect.multiplier
        );
    }

    #[test]
    fn future_author_decisions_are_dropped_before_ci() {
        let mut rows = fixed_decisions(true);
        for row in &mut rows {
            row.at_ms = 50_000;
        }
        let author = AuthorObservations {
            declared: None,
            decisions: rows,
        };
        assert!(evidence_from_observations(10_000, 1_000, &[], Some(&author)).is_none());
    }

    #[test]
    fn empty_observations_are_absent() {
        assert!(evidence_from_observations(10_000, 1_000, &[], None).is_none());
    }
}
