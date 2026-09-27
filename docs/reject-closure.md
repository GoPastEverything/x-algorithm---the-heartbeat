# Reject Closure

Product contract. Not a bot classifier and not part of the PULSE
estimator.

If viewer `u` blocks actor `a` at time `t`:

```text
B_u(a, t) = 1
```

then for every later time `t' > t`:

```text
Candidate(a, u, t')            = 0
Suggestion(a, u, t')           = 0
Influence(a \u2192 u, t')           = 0
```

`AuthorSocialgraphFilter` already implements the candidate drop for For
You: blocked and muted authors, quotes and retweets of them, and the
reverse block.

PULSE implements the influence drop for the lagged field only:

```text
P_u(E, t) = P(E, t) \ B_u
```

Blocked and muted actor IDs are stripped in `PulseHydrator` before
`evidence_from_observations`. The global source is unchanged.

## Surfaces that must share the exclusion set

| Surface | In this repo | Status |
|---|---|---|
| For You author | yes | `AuthorSocialgraphFilter` |
| For You lagged field | yes | `viewer_influence_mask` |
| Out-of-network discovery of *other* authors | yes | kept. OON is not a ban |
| @ typeahead | no | product contract only |
| Who to Follow | no | product contract only |
| Search | no | product contract only |

Out-of-network posts from accounts the viewer simply does not know stay
eligible. A block is not "never interacted."

## Reject Attribution (hypothesis)

`profile_click(u, a, t1)` followed by `block(u, a, t2)` with
`0 < t2 \u2212 t1 < \u0394` is one rejection episode, not an interest click plus a
later block.

The public tree proves Phoenix has a `profile_click` head and a
`block_author` head. It does not prove production event construction
keeps the click after the block. Do not describe that as a confirmed
bug. It is a sequence experiment for a later slice, and it does not
belong on `PulseEventSource`.
