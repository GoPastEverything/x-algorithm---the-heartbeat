# PULSE in Home Mixer

A causal liveness prior on For You ranking. Off by default.

Phoenix / the value model still produce
`weighted_score = Σ wᵢ P̂(actionᵢ)` (or the VM ranker's equivalent).
PULSE multiplies only the final `score`:

```text
score' = score · q_t
q_t    = q(L_actor at t-, P_u(E, [t−W, t)))
```

`weighted_score` is left alone so the published action-head sum stays
auditable.

This tree is rebased onto `xai-org/x-algorithm` `4c5cfe8` (2026-09-26).
The old `RankingScorer` is gone. `q_t` is applied in
`PhoenixScoresRankingScorer` and at the end of `VMRanker`, after that
scorer has written `score`.

## Ranking

- `home-mixer/models/pulse.rs` — `PulseEvidence`, `causal_q`, and the
  estimators below.
- `PostCandidate.pulse` — optional, `serde(default)`. Absent means identity.
- `EnablePulsePrior` / `rust_home_mixer_enable_pulse_prior`, default `false`.
- Flag off → `q = 1`.
- `pulse` missing → `q = 1`. Cold start is not a penalty.
- `sigma > 0.18` → `q = 1`.
- `lagged_n < 4` → author-side state only.
- `lagged_*` is reactions with timestamp `< t`. Future events are a bug.

There is no public liveness score, no threshold, and no user-facing copy.

## Viewer influence mask

The event source answers "what reactions already happened?"
The hydrator then subtracts actors this viewer blocked or muted:

```text
P_u(E, t) = P(E, t) \ B_u
```

A blocked actor is not labeled a bot. They simply cannot write into this
viewer's lagged field. See [reject-closure.md](reject-closure.md).

That mask cannot move rank in this slice. Unlabeled posteriors stay at
`sigma ≥ 0.22` and the gate is `0.18`. The invariant is encoded now so a
later slice that is allowed to narrow `sigma` does not inherit blocked
actors.

## Lagged events

`lagged_field` keeps reactions with `scoring_time - W <= at_ms < scoring_time`.
`W` is `LAGGED_WINDOW_MS` (30 minutes). Counts (`fav_count` and the rest)
are not a window and are not read.

`counterfactual_independence` is the time-split log-likelihood lift of a
lagged cluster residual over stimulus, own rate, and feed rate. High CI
is evidence of a shared unobserved controller. It is not a bot verdict.
The name overclaims; it is residual predictive lift, not a randomized
counterfactual.

What is allowed to narrow `sigma` under 0.18 in this slice:

- an explicit `DeclaredClass::Agent` or `DeclaredClass::Service`

What is not:

- a reaction field, however synchronized
- a high unlabeled CI
- `DeclaredClass::Human`

Unlabeled posteriors are floored at `sigma = 0.22`.

## Hydrator

`PulseHydrator` is registered on `PhoenixCandidatePipeline` with
`EmptyPulseEventSource`. `EnablePulseHydrator` defaults to `false`.
Viewer-centric user-action sequences and lifetime engagement totals are
the wrong shape for `PulseEventSource`. Source errors fail open.

## What this still does not do

No production client implements `PulseEventSource`. Turning either flag
on in this tree does not change For You. Mention typeahead, Who to Follow,
and search are other surfaces; they should consume the same exclusion
set, but they are not in this repository.

Reject Attribution (`profile_click → block` inside a session window) is a
Phoenix sequence hypothesis, not a proven production bug and not part of
this slice.
