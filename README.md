# X For You — PULSE fork

This is **not** the official [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm) tree.

It is a public fork of that snapshot, plus a causal **PULSE** (Human Liveness Signal) prior that can multiply the final For You score. PULSE is off by default. Turning nothing on leaves ranking identical to upstream.

| | |
|---|---|
| Upstream | [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm) `@4c5cfe8` (2026-09-26) |
| This repo | [GoPastEverything/x-algorithm---the-heartbeat](https://github.com/GoPastEverything/x-algorithm--the-heartbeat) |
| Live lab (docs + sim) | [xalgorithm.grok.me](https://xalgorithm.grok.me) |
| Current PULSE branch | `pulse/causal-q-4c5cfe8` |
| Current PULSE PR | [#3](https://github.com/GoPastEverything/x-algorithm---the-heartbeat/pull/3) (draft) |
| License | Apache-2.0, same as upstream |

If you opened this thinking it *is* X's algorithm: the ranking, retrieval, filters, and visibility-filtering code here *are* that algorithm, mirrored. The PULSE files and the two feature flags are the only additive contract. They do not ship in production X.

The full breakdown — contract, sigma gate, reject closure, file map, flags, and what is still missing — lives on the PULSE branch README so it stays next to the code it describes:

**[README on `pulse/causal-q-4c5cfe8`](https://github.com/TheRetardedElon/x-algorithm---the-heartbeat/blob/pulse/causal-q-4c5cfe8/README.md)**

Also:

- [docs/pulse.md](https://github.com/TheRetardedElon/x-algorithm---the-heartbeat/blob/pulse/causal-q-4c5cfe8/docs/pulse.md)
- [docs/reject-closure.md](https://github.com/TheRetardedElon/x-algorithm---the-heartbeat/blob/pulse/causal-q-4c5cfe8/docs/reject-closure.md)
- [FORK.md](https://github.com/TheRetardedElon/x-algorithm---the-heartbeat/blob/pulse/causal-q-4c5cfe8/FORK.md)
- Draft PR: [#3](https://github.com/TheRetardedElon/x-algorithm---the-heartbeat/pull/3)

`main` here is a sync of upstream `@4c5cfe8` (merged as [PR #2](https://github.com/TheRetardedElon/x-algorithm---the-heartbeat/pull/2)). It does not contain the PULSE mixer files. Look at the branch above for those.

Official X algorithm documentation remains at [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm/blob/main/README.md).
