# X For You — PULSE fork

This is **not** the official [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm) tree.

It is a public fork of that snapshot, plus a causal **PULSE** (Human Liveness Signal) prior that can multiply the final For You score. PULSE is off by default. Turning nothing on leaves ranking identical to upstream.

| | |
|---|---|
| Upstream | [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm) `@4c5cfe8` (2026-09-26) |
| This repo | [TheRetardedElon/x-algorithm---TheRetardedElon](https://github.com/TheRetardedElon/x-algorithm---TheRetardedElon) |
| Live lab (docs + sim) | [xalgorithm.grok.me](https://xalgorithm.grok.me) |
| Current PULSE branch | `pulse/causal-q-4c5cfe8` |
| Current PULSE PR | [#3](https://github.com/TheRetardedElon/x-algorithm---TheRetardedElon/pull/3) (draft) |
| License | Apache-2.0, same as upstream |

If you opened this thinking it *is* X's algorithm: the ranking, retrieval, filters, and visibility-filtering code here *are* that algorithm, mirrored. The PULSE files and the two feature flags are the only additive contract. They do not ship in production X.

---

## Table of contents

1. [What this fork is trying to do](#1-what-this-fork-is-trying-to-do)
2. [Read this first — the contract](#2-read-this-first--the-contract)
3. [Status right now](#3-status-right-now)
4. [Branches and pull requests](#4-branches-and-pull-requests)
5. [How For You works in this snapshot](#5-how-for-you-works-in-this-snapshot)
6. [Where PULSE sits](#6-where-pulse-sits)
7. [The sigma gate](#7-the-sigma-gate)
8. [Viewer influence mask / Reject Closure](#8-viewer-influence-mask--reject-closure)
9. [What is not PULSE](#9-what-is-not-pulse)
10. [File map](#10-file-map)
11. [Feature flags](#11-feature-flags)
12. [What is still missing](#12-what-is-still-missing)
13. [How to review this](#13-how-to-review-this)
14. [Upstream For You (unchanged)](#14-upstream-for-you-unchanged)
15. [License](#15-license)

---

## 1. What this fork is trying to do

For You already predicts how likely *you* are to like, reply, dwell, block, mute, or report a post. Those heads are Phoenix. Their weighted sum is the published score.

That sum does not ask a different question: **is the activity on this post a lagged field of independently acting humans, or a coupled controller?**

PULSE is that second question, as a *prior*, not as a public badge and not as a rewrite of Phoenix.

```text
Phoenix / value model writes   weighted_score = Σ wᵢ P̂(actionᵢ)
then the final score becomes   score' = score · q_t
```

`q_t` is 1 unless two things are true at once:

- the rank flag is on, and
- the posterior on the author + lagged field is *narrow* (`σ ≤ 0.18`)

A wide interval is identity. A missing field is identity. A cold start is identity. A synchronized swarm that is still unlabeled stays identity. Only a narrow posterior is allowed to discount.

The point of that gate: newsrooms, breaking news, sports, and fan accounts look coordinated. Treating coordination as guilt would punish real humans. The first slice that may actually move rank is an **explicitly declared agent or service**. Everything else is observational until an offline harness says unlabeled evidence can narrow `σ` without a brutal false-positive rate.

PULSE is private. There is no user-visible liveness number, no threshold copy, and no appeal text that describes the detector.

Longer writeup: [docs/pulse.md](docs/pulse.md).

---

## 2. Read this first — the contract

```text
q_t = 1     if EnablePulsePrior is false
q_t = 1     if candidate.pulse is missing
q_t = 1     if sigma is missing, NaN, or > 0.18
q_t = q(·)  only if the posterior is narrow

weighted_score is never rewritten.
fav_count / reply_count / view_count are not inputs.
Reactions with at_ms >= scoring_time are dropped.
Viewer UAS (the viewer's own click stream) is the wrong shape
for PulseEventSource. The source is reactions on the post
and past decisions by the author being ranked.
```

Taxonomy the estimator talks about:

| Class | What it means | Can narrow `σ` in this slice? |
|---|---|---|
| human | independently acting person | no |
| mixed | human + tools | no |
| agent | declared model / automation | yes (`σ = 0.08`) |
| service | declared service account | yes (`σ = 0.08`) |
| coordinated | coupled actors, not a verdict | no |

Unlabeled posteriors are floored at `σ = 0.22`, which is *above* the action gate. High CI on an unlabeled swarm is a measurement, not a rank change.

---

## 3. Status right now

Honest snapshot as of 2026-09-26.

| Item | State |
|---|---|
| Fork `main` vs upstream | Synced. PR #2 merged `4c5cfe8` into this `main`. |
| PULSE on by default | **No.** Both flags default `false`. |
| Production event source | **None.** `EmptyPulseEventSource` returns empty windows. |
| Does this change live For You on X? | **No.** |
| Can `q_t` move a score in this tree? | Only if someone turns the flags on *and* attaches narrow evidence by hand. Declared agent/service is the only class that produces that evidence today. |
| Old `RankingScorer` | **Gone upstream.** PULSE no longer hooks there. |
| Lab site | [xalgorithm.grok.me](https://xalgorithm.grok.me) — documentation + causal simulation. The lab is not this Rust tree. |

The mixer still needs three small wirings on `pulse/causal-q-4c5cfe8` before a home-mixer build would see `PostCandidate.pulse` and the flags: the field on the candidate struct, the two `param!` flags, and `PulseHydrator::empty()` in the Phoenix hydrator list. The estimator, hydrator, mask, and both scorer hooks are already on that branch.

---

## 4. Branches and pull requests

```text
xai-org/x-algorithm                 main @ 4c5cfe8   2026-09-26
        │
        │  PR #2 (merged)
        ▼
this repo main                      synced snapshot, no PULSE
        │
        ├── pulse/causal-q          original 5 commits on the Sept 1
        │                           snapshot. Hooks RankingScorer.
        │                           SUPERSEDED. Do not patch this branch.
        │                           PR #1 is the record of that work.
        │
        └── pulse/causal-q-4c5cfe8  current work. PR #3.
                                    Rebased onto 4c5cfe8.
                                    Hooks PhoenixScoresRankingScorer
                                    and VMRanker.
```

| PR | What it is |
|---|---|
| [#2](https://github.com/TheRetardedElon/x-algorithm---TheRetardedElon/pull/2) | Sync `main` with upstream. Merged. |
| [#1](https://github.com/TheRetardedElon/x-algorithm---TheRetardedElon/pull/1) | First PULSE slice, Sept 1 tree. Draft, superseded. |
| [#3](https://github.com/TheRetardedElon/x-algorithm---TheRetardedElon/pull/3) | Current PULSE + viewer mask on the new scorers. Draft. |

Why two PULSE branches: between Sept 1 and Sept 26 upstream deleted `home-mixer/scorers/ranking_scorer.rs` and moved the published sum into `value_model.rs` + `VMRanker`. Replaying the old five commits on the new tree would conflict on a file that no longer exists. The estimator math moved forward. The hook moved with the score.

---

## 5. How For You works in this snapshot

Unchanged from upstream. Short version:

1. **Query hydration** loads the viewer's follow list, blocks, mutes, muted keywords, and recent action sequence.
2. **Sources** run in parallel:
   - in-network: `thunder/` (accounts you follow)
   - out-of-network: Phoenix retrieval + SimClusters
3. **Candidate hydration** attaches text, media, author labels, engagement *counts* (those counts are display/filter features, not the PULSE window).
4. **Pre-scoring filters** drop duplicates, old posts, self posts, blocked/muted authors, muted keywords, already-seen, etc. `AuthorSocialgraphFilter` is the hard block/mute drop.
5. **Scoring**
   - `PhoenixScorer` writes action-head probabilities
   - `value_model` / `PhoenixScoresRankingScorer` fuse those heads into `weighted_score`
   - `VMRanker` may replace `score` with a remote rerank; on RPC failure it keeps the local weighted score
6. **Top-K** by `score`
7. **Visibility filtering** after the order is fixed

Out-of-network discovery is intentional. A stranger can still appear in For You. That is not a bug and PULSE does not ban it. OON is discounted, not deleted.

Blocks already drop *authors* from the candidate list. They do not, in upstream, drop that actor from being a *reactor* on someone else's post. That gap is what the viewer mask is for.

---

## 6. Where PULSE sits

```text
                 reactions already arrived on the post
                 + author decisions already observed
                              │
                              ▼
                    PulseEventSource
                    (empty in this tree)
                              │
                              ▼
                    viewer_influence_mask
                    drop actors in B_u ∪ M_u
                              │
                              ▼
                 evidence_from_observations
                 → PulseEvidence { L, σ, I, CI, lagged_* }
                              │
                              ▼
                 candidate.pulse   (optional)
                              │
          ┌──────────────────┐
          ▼                                       ▼
 PhoenixScoresRankingScorer                    VMRanker
 score = weighted * q_t                   score = score * q_t
 weighted_score untouched                 weighted_score untouched
```

`q_t = causal_multiplier(EnablePulsePrior, candidate.pulse)`.

The hydrator is registered only in spirit until the Phoenix pipeline list is wired. The empty source means even a wired hydrator with the flag on still leaves `pulse` unset, so ranking stays identity.

---

## 7. The sigma gate

```text
                 σ
                 │
     0.08 ───────┼── declared agent / service   → q_t may discount
                 │
     0.18 ───────┼── ACTION GATE
                 │     above this line, q_t is exactly 1
                 │
     0.22 ───────┼── unlabeled / declared-human floor
                 │
     0.30 ───────┼── too few author decisions
```

That picture is the whole safety argument for v0.

- Observe a swarm: yes.
- Measure CI: yes.
- Attach evidence: yes.
- Change rank from unlabeled coordination: **no**.
- Change rank from a declared agent: **yes**, and only then.

`counterfactual_independence` on this branch is residual predictive lift of a lagged cluster term. The name overclaims. It is not a randomized counterfactual. Newsrooms will light it up. That is why CI alone cannot narrow `σ`.

---

## 8. Viewer influence mask / Reject Closure

Two different systems. Do not fold them.

### PULSE — network authenticity

Author history + lagged reaction field + residual coordination → `PulseEvidence` → sigma gate → `q_t`.

### Reject Closure — preference isolation

If viewer `u` blocks actor `a` at time `t`:

```text
B_u(a, t) = 1

then for t' > t:
  Candidate(a, u, t')   = 0     already true in AuthorSocialgraphFilter
  Suggestion(a, u, t')  = 0     product contract, other surfaces
  Influence(a → u, t')  = 0     viewer_influence_mask on the lagged field
```

```text
P_u(E, t) = P(E, t) \ B_u
```

A blocked account is not labeled a bot. They are excluded from *this viewer's* field. The global source still returns every arrived reaction. The hydrator subtracts.

| Surface | In this repo | What happens after a block |
|---|---|---|
| For You author | yes | hard drop (`AuthorSocialgraphFilter`) |
| For You lagged field | yes | `viewer_influence_mask` |
| OON posts from people you simply don't know | yes | kept. OON is not a ban |
| @ typeahead | **no** | same exclusion set should apply; compose is another service |
| Who to Follow | **no** | same |
| Search | **no** | same |

Full writeup: [docs/reject-closure.md](docs/reject-closure.md).

### Reject Attribution (hypothesis, not shipped)

`profile_click(u, a, t1)` then `block(u, a, t2)` inside a short window is one rejection episode, not an interest click plus a later block.

The public tree proves Phoenix has a `profile_click` head and a `block_author` head. It does **not** prove production event construction keeps the click after the block. Do not file that as a confirmed X bug. It is a sequence experiment for a later slice, and it does not belong on `PulseEventSource`.

---

## 9. What is not PULSE

- A public "human" badge
- A binary human/bot classifier
- A rewrite of Phoenix's weighted sum
- A ban on out-of-network retrieval
- Mention suggestions, Who to Follow, or search
- Punishment of unlabeled coordination
- An appeal flow that explains the detector (that would be a training set)

---

## 10. File map

PULSE files on `pulse/causal-q-4c5cfe8`:

| Path | Role |
|---|---|
| [home-mixer/models/pulse.rs](home-mixer/models/pulse.rs) | `PulseEvidence`, `causal_q`, sigma policy, lagged field, `viewer_influence_mask` |
| [home-mixer/candidate_hydrators/pulse_hydrator.rs](home-mixer/candidate_hydrators/pulse_hydrator.rs) | `PulseEventSource` trait, empty source, mask, attach evidence |
| [home-mixer/scorers/phoenix_scores_ranking_scorer.rs](home-mixer/scorers/phoenix_scores_ranking_scorer.rs) | `score = weighted * q_t` |
| [home-mixer/scorers/vm_ranker.rs](home-mixer/scorers/vm_ranker.rs) | `apply_pulse` after remote or local score |
| [home-mixer/models/mod.rs](home-mixer/models/mod.rs) | `pub mod pulse` |
| [home-mixer/candidate_hydrators/mod.rs](home-mixer/candidate_hydrators/mod.rs) | `pub mod pulse_hydrator` |
| [docs/pulse.md](docs/pulse.md) | mixer contract |
| [docs/reject-closure.md](docs/reject-closure.md) | block / mute / suggestion contract |
| [FORK.md](FORK.md) | branch pointer |

Upstream files PULSE reads but does not own:

| Path | Role |
|---|---|
| `home-mixer/filters/author_socialgraph_filter.rs` | hard-drop blocked/muted *authors* |
| `home-mixer/models/user_features.rs` | `blocked_user_ids`, `muted_user_ids` |
| `home-mixer/scorers/value_model.rs` | published `weighted_score` |
| `home-mixer/candidate_pipeline/phoenix_candidate_pipeline.rs` | hydrator / scorer order |

The full OLS CI estimator from the Sept 1 slice still lives on `pulse/causal-q` (`counterfactual_independence` with time-split lift). The 4c5cfe8 copy keeps the same *gate* and a simpler lift. Do not treat the name as a randomized experiment.

---

## 11. Feature flags

| Flag | Default | Effect |
|---|---|
| `rust_home_mixer_enable_pulse_prior` | `false` | allow `q_t ≠ 1` when evidence is narrow |
| `rust_home_mixer_enable_pulse_hydrator` | `false` | run `PulseHydrator` |

Both must be on *and* a real `PulseEventSource` must exist before For You can change. The source in this tree is empty. Source errors fail open (`pulse` stays unset → `q_t = 1`).

---

## 12. What is still missing

In priority order:

1. Wire `PostCandidate.pulse`, the two flags, and `PulseHydrator::empty()` on the Phoenix pipeline. Without those, a home-mixer build on this branch will not compile the new modules into the request path.
2. A real `PulseEventSource` that returns per-post reactions with timestamps `< t` and optional author decision rows. Not viewer UAS. Not lifetime counts.
3. Offline falsification harness before unlabeled evidence is allowed to narrow `σ`: organic humans, breaking news, sports, newsrooms, fan communities, declared bots, independent agents, centrally controlled swarms, aged-account swarms. Measure `P(false positive | legitimate coordination)`.
4. Cross-surface Reject Closure for @ typeahead, Who to Follow, and search. Those services are not in this repository.
5. Reject Attribution as a Phoenix sequence experiment, labeled as a hypothesis.
6. Rename `counterfactual_independence` to residual coordination lift when a randomized-exposure estimator actually exists.

---

## 13. How to review this

Start here, in this order:

1. This README, then [docs/pulse.md](docs/pulse.md).
2. `causal_multiplier` and `posterior_sigma` in `home-mixer/models/pulse.rs`.
3. `viewer_influence_mask` and the hydrator's excluded-id list.
4. The two `q_t` call sites. Confirm `weighted_score` is not multiplied.
5. `AuthorSocialgraphFilter` so you can see the author-drop that already existed.

Ignore `pulse/causal-q` unless you want the original commit history.

This snapshot cannot `cargo test` home-mixer in a public checkout. Private crates (`xai_feature_switches`, `xai_candidate_pipeline`, and the rest) are not in the open tree. Unit tests in `pulse.rs` are the contract. They need those crates to run.

---

## 14. Upstream For You (unchanged)

Everything below is the mirrored X algorithm, not PULSE.

- In-network store: [`thunder/`](thunder/)
- Retrieval + rank model training: [`phoenix/`](phoenix/)
- Graph / community retrieval: [`simclusters/`](simclusters/)
- Mixer: [`home-mixer/`](home-mixer/)
- Visibility filtering: [`visibility-filtering/`](visibility-filtering/)
- Label producers: [`botmaker/`](botmaker/), [`scarecrow/`](scarecrow/), [`agatha/`](agatha/), [`bdsm/`](bdsm/)
- Action-head weights and the "weights are not raw counts" comments: [`home-mixer/params/param.rs`](home-mixer/params/param.rs)

Official notable-updates and the Under the Hood transparency tool live in the [upstream README](https://github.com/xai-org/x-algorithm/blob/main/README.md). This fork does not replace that documentation. It adds a prior on top of the published score.

What is *also* not in either public tree: mention typeahead, the production event log that would feed `PulseEventSource`, ads ranking internals beyond the blender hook, and the live Phoenix checkpoints.

---

## 15. License

Apache License 2.0. Same as [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm).
