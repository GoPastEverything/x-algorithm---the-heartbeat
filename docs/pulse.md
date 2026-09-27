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

## Ranking

- `home-mixer/models/pulse.rs` — `PulseEvidence`, `causal_q`, and the
  estimators below.
- `PostCandidate.pulse` — optional, `serde(default)`. Absent means identity.
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

There is no public liveness score, no threshold, and no user-facing copy.

## Lagged events

`lagged_field` keeps reactions with `scoring_time - W <= at_ms < scoring_time`.
`W` is `LAGGED_WINDOW_MS` (30 minutes). Counts (`fav_count` and the rest)
are not a window and are not read.

`counterfactual_independence` is the time-split log-likelihood lift of a
lagged cluster residual over stimulus, own rate, and feed rate. High CI is
evidence of a shared unobserved controller. It is not a bot verdict.
Newsrooms and breaking-news clusters will light it up.

`class_posterior` is a mixture over human, mixed, agent, service, and
coordinated. `liveness_from_posterior` turns that into one number. It is
not `P(human)`.

What is allowed to narrow `sigma` under 0.18 in this slice:

- an explicit `DeclaredClass::Agent` or `DeclaredClass::Service`

What is not:

- a reaction field, however synchronized
- a high unlabeled CI
- `DeclaredClass::Human` (a human label must not rank below "no label")

Unlabeled posteriors are floored at `sigma = 0.22`.

## Hydrator

`PulseHydrator` is registered on `PhoenixCandidatePipeline` with
`EmptyPulseEventSource`. `EnablePulseHydrator` /
`rust_home_mixer_enable_pulse_hydrator` defaults to `false`. With the flag
on, the empty source still leaves `pulse` unset.

A real `PulseEventSource` has to return per-post reactions and optional
author decisions that already happened. Viewer-centric user-action
sequences and lifetime engagement totals are the wrong shape. Source
errors fail open: ranking stays unchanged.

`update` only writes `pulse` when the new value is `Some`, so a miss does
not erase evidence already on the candidate.

## What this still does not do

No production client implements `PulseEventSource`. Turning either flag on
in this tree does not change For You. There is no offline experiment
harness against live traffic, and no appeal surface. An appeal that
describes the detector would be a training set for the next swarm.

## How to exercise it

Unit tests:

- `models/pulse.rs` — window cutoff, swarm vs trickle, CI lift, and the
  cases that must not move rank
- `candidate_hydrators/pulse_hydrator.rs` — flag, empty source, future
  events, counts left alone
- `RankingScorer::pulse_prior_is_a_noop_until_evidence_is_confident` —
  wide `sigma` is identity; a hand-attached narrow posterior discounts
  `score` only

They need a home-mixer build. The public snapshot does not include the
private crates those tests link.
