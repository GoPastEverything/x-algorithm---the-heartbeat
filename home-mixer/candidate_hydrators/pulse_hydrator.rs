//! Attaches [`crate::models::pulse::PulseEvidence`] from a lagged event source.
//!
//! The registered source is empty. Engagement counts are not events: this
//! hydrator never reads `fav_count` or any other total. A source error or an
//! empty window leaves `candidate.pulse` unset, which ranking treats as 1.

use crate::models::candidate::{CandidateHelpers, PostCandidate};
use crate::models::pulse::{
    evidence_from_observations, AuthorObservations, ReactionEvent, LAGGED_WINDOW_MS,
};
use crate::models::query::ScoredPostsQuery;
use crate::params::EnablePulseHydrator;
use std::sync::Arc;
use tonic::async_trait;
use tracing::warn;
use xai_candidate_pipeline::hydrator::Hydrator;

#[async_trait]
pub trait PulseEventSource: Send + Sync {
    /// Reactions with `at_ms < scoring_time_ms` only. One vec per post id.
    async fn reactions_for_posts(
        &self,
        post_ids: &[u64],
        scoring_time_ms: u64,
    ) -> Result<Vec<Vec<ReactionEvent>>, String>;

    /// Author-side rows already observed. One entry per author id.
    async fn author_observations(
        &self,
        author_ids: &[u64],
        scoring_time_ms: u64,
    ) -> Result<Vec<Option<AuthorObservations>>, String>;
}

pub struct EmptyPulseEventSource;

#[async_trait]
impl PulseEventSource for EmptyPulseEventSource {
    async fn reactions_for_posts(
        &self,
        post_ids: &[u64],
        _scoring_time_ms: u64,
    ) -> Result<Vec<Vec<ReactionEvent>>, String> {
        Ok(vec![Vec::new(); post_ids.len()])
    }

    async fn author_observations(
        &self,
        author_ids: &[u64],
        _scoring_time_ms: u64,
    ) -> Result<Vec<Option<AuthorObservations>>, String> {
        Ok(vec![None; author_ids.len()])
    }
}

pub struct PulseHydrator {
    source: Arc<dyn PulseEventSource>,
}

impl PulseHydrator {
    pub fn new(source: Arc<dyn PulseEventSource>) -> Self {
        Self { source }
    }

    pub fn empty() -> Self {
        Self::new(Arc::new(EmptyPulseEventSource))
    }
}

#[async_trait]
impl Hydrator<ScoredPostsQuery, PostCandidate> for PulseHydrator {
    fn enable(&self, query: &ScoredPostsQuery) -> bool {
        query.params.get(EnablePulseHydrator) && !query.has_cached_posts
    }

    async fn hydrate(
        &self,
        query: &ScoredPostsQuery,
        candidates: &[PostCandidate],
    ) -> Vec<Result<PostCandidate, String>> {
        let scoring_time_ms = u64::try_from(query.request_time_ms).unwrap_or(0);
        let post_ids: Vec<u64> = candidates
            .iter()
            .map(|candidate| candidate.get_original_tweet_id())
            .collect();
        let author_ids: Vec<u64> = candidates
            .iter()
            .map(|candidate| candidate.author_id)
            .collect();

        let reactions = match self
            .source
            .reactions_for_posts(&post_ids, scoring_time_ms)
            .await
        {
            Ok(batches) if batches.len() == candidates.len() => batches,
            Ok(_) => {
                warn!("pulse hydrator: reaction batch length mismatch; leaving pulse unset");
                vec![Vec::new(); candidates.len()]
            }
            Err(error) => {
                warn!(error = %error, "pulse hydrator: reaction source failed; leaving pulse unset");
                vec![Vec::new(); candidates.len()]
            }
        };
        let authors = match self
            .source
            .author_observations(&author_ids, scoring_time_ms)
            .await
        {
            Ok(batches) if batches.len() == candidates.len() => batches,
            Ok(_) => {
                warn!("pulse hydrator: author batch length mismatch; ignoring author side");
                vec![None; candidates.len()]
            }
            Err(error) => {
                warn!(error = %error, "pulse hydrator: author source failed; ignoring author side");
                vec![None; candidates.len()]
            }
        };

        reactions
            .into_iter()
            .zip(authors)
            .map(|(events, author)| {
                Ok(PostCandidate {
                    pulse: evidence_from_observations(
                        scoring_time_ms,
                        LAGGED_WINDOW_MS,
                        &events,
                        author.as_ref(),
                    ),
                    ..Default::default()
                })
            })
            .collect()
    }

    fn update(&self, candidate: &mut PostCandidate, hydrated: PostCandidate) {
        if hydrated.pulse.is_some() {
            candidate.pulse = hydrated.pulse;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::pulse::{rank_effect, ReactionEvent, ReactionKind, LAGGED_WINDOW_MS};

    const FLAG: &str = "rust_home_mixer_enable_pulse_hydrator";

    fn query(flag: bool, cached: bool, request_time_ms: i64) -> ScoredPostsQuery {
        let mut query = ScoredPostsQuery {
            has_cached_posts: cached,
            request_time_ms,
            ..Default::default()
        };
        let switches = xai_feature_switches::FeatureSwitches::new(vec![]).unwrap();
        let mut results =
            switches.match_recipient(&xai_feature_switches::RecipientBuilder::new().build());
        if flag {
            results.override_fs(FLAG.to_string(), "true");
        }
        query.params = results.into();
        query
    }

    struct ScriptedSource {
        reactions: Vec<ReactionEvent>,
        fail: bool,
    }

    #[async_trait]
    impl PulseEventSource for ScriptedSource {
        async fn reactions_for_posts(
            &self,
            post_ids: &[u64],
            _scoring_time_ms: u64,
        ) -> Result<Vec<Vec<ReactionEvent>>, String> {
            if self.fail {
                return Err("source down".to_string());
            }
            Ok(post_ids.iter().map(|_| self.reactions.clone()).collect())
        }

        async fn author_observations(
            &self,
            author_ids: &[u64],
            _scoring_time_ms: u64,
        ) -> Result<Vec<Option<AuthorObservations>>, String> {
            Ok(vec![None; author_ids.len()])
        }
    }

    #[test]
    fn enable_is_off_until_the_flag_and_skips_cached_posts() {
        let hydrator = PulseHydrator::empty();
        assert!(!hydrator.enable(&query(false, false, 1)));
        assert!(!hydrator.enable(&query(true, true, 1)));
        assert!(hydrator.enable(&query(true, false, 1)));
    }

    #[tokio::test]
    async fn empty_source_does_not_read_counts_or_set_pulse() {
        let hydrator = PulseHydrator::empty();
        let mut candidate = PostCandidate {
            tweet_id: 7,
            author_id: 3,
            fav_count: Some(9_999),
            ..Default::default()
        };
        let hydrated = hydrator
            .hydrate(
                &query(true, false, 5_000_000),
                std::slice::from_ref(&candidate),
            )
            .await;
        let partial = hydrated[0].as_ref().unwrap();
        assert!(partial.pulse.is_none());
        assert!(partial.fav_count.is_none());
        hydrator.update(&mut candidate, partial.clone());
        assert_eq!(candidate.fav_count, Some(9_999));
        assert!(candidate.pulse.is_none());
    }

    #[tokio::test]
    async fn source_failure_and_future_events_leave_pulse_unset() {
        let failing = PulseHydrator::new(Arc::new(ScriptedSource {
            reactions: Vec::new(),
            fail: true,
        }));
        let candidate = PostCandidate {
            tweet_id: 1,
            ..Default::default()
        };
        let failed = failing
            .hydrate(&query(true, false, 5_000), std::slice::from_ref(&candidate))
            .await;
        assert!(failed[0].as_ref().unwrap().pulse.is_none());

        let scoring = 5_000_000u64;
        let future = PulseHydrator::new(Arc::new(ScriptedSource {
            reactions: vec![ReactionEvent {
                actor_id: 9,
                at_ms: scoring,
                kind: ReactionKind::Favorite,
                cluster_id: Some(1),
            }],
            fail: false,
        }));
        let dropped = future
            .hydrate(
                &query(true, false, scoring as i64),
                std::slice::from_ref(&candidate),
            )
            .await;
        assert!(dropped[0].as_ref().unwrap().pulse.is_none());
    }

    #[tokio::test]
    async fn in_window_burst_is_attached_and_does_not_move_rank() {
        let scoring = 5_000_000u64;
        let reactions: Vec<ReactionEvent> = (0..8)
            .map(|i| ReactionEvent {
                actor_id: 100 + i,
                at_ms: scoring - 1_000 - i,
                kind: ReactionKind::Favorite,
                cluster_id: Some(2),
            })
            .collect();
        assert!(reactions
            .iter()
            .all(|event| scoring - event.at_ms < LAGGED_WINDOW_MS));
        let hydrator = PulseHydrator::new(Arc::new(ScriptedSource {
            reactions,
            fail: false,
        }));
        let mut candidate = PostCandidate {
            tweet_id: 42,
            author_id: 8,
            fav_count: Some(1),
            ..Default::default()
        };
        let hydrated = hydrator
            .hydrate(
                &query(true, false, scoring as i64),
                std::slice::from_ref(&candidate),
            )
            .await;
        let pulse = hydrated[0].as_ref().unwrap().pulse.unwrap();
        assert_eq!(pulse.lagged_n, 8);
        assert!(pulse.sigma > 0.18);
        assert!(!rank_effect(true, &pulse).moves);
        hydrator.update(&mut candidate, hydrated[0].as_ref().unwrap().clone());
        assert_eq!(candidate.fav_count, Some(1));
        assert_eq!(candidate.pulse.unwrap().lagged_n, 8);
    }
}
