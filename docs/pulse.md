# PULSE in Home Mixer

A causal liveness prior on For You ranking. Off by default.

Phoenix still produces `weighted_score = Σ wᵢ P̂(actionᵢ)`. PULSE multiplies
only the final `score`, after author diversity and the out-of-network
discount:

```text
score' = score · q_t
q_t    = q(L_actor at t-, P(E, [t−W, t)))
```

`weighted_score` is left alone so the published action-head sum stays
auditable.

## What this commit does

- `home-mixer/models/pulse.rs` — `PulseEvidence` and `causal_q`.
- `PostCandidate.pulse` — optional, `serde(default)`. Absent on every
  existing candidate.
- `EnablePulsePrior` / `rust_home_mixer_enable_pulse_prior`, default `false`.
- `RankingScorer` applies `q_t` on both the pre-offset and post-offset paths.

Rules baked into the multiplier:

- Flag off → `q = 1`.
- `pulse` missing → `q = 1`. Cold start is not a penalty.
- `sigma > 0.18` → `q = 1`. A point estimate without a narrow interval
  does not touch ranking.
- `lagged_n < 4` → author-side state only. The reaction field of the
  current event is not used before it exists.
- `lagged_*` is defined as reactions with timestamp `< t`. A hydrator that
  fills these from the future is a bug.

There is no public liveness score, no threshold, and no user-facing copy
in this change.

## What this commit does not do

No hydrator writes `PulseEvidence` yet. With the flag on and no features
attached, ranking is identical to today. The next slice is a hydrator that
fills lagged event stats from engagement that has already arrived, plus an
offline ΔL report. Counterfactual independence (`ci`) is an input, not an
estimator: the estimator is predictive lift of the cluster residual after
the stimulus the account actually saw.

High `ci` is evidence of a shared unobserved controller. It is not a bot
verdict. Newsrooms and breaking-news clusters will light it up; the class
posterior has to type them before `q_t` should move.

## How to exercise it

The unit tests in `models/pulse.rs` and
`RankingScorer::pulse_prior_is_a_noop_until_evidence_is_confident` cover
the contract without a running mixer. Turning the flag on in an environment
does nothing until a hydrator sets `candidate.pulse`.
