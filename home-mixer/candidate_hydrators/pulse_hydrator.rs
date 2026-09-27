//! Attaches PulseEvidence from a lagged event source.
//!
//! The registered source is empty. Engagement counts are not events.
//! Viewer block/mute lists are subtracted before evidence_from_observations.

use crate::models::candidate::{CandidateHelpers, PostCandidate};
use crate::models::pulse::{
    evidence_from_observations, viewer_influence_mask, AuthorObservations, ReactionEvent,
    LAGGED_WINDOW_MS,
};
use crate::models::query::ScoredPostsQuery;
use crate::params::EnablePulseHydrator;
use std::sync::Arc;
use tonic::async_trait;
use tracing::warn;
use xai_candidate_pipeline::hydrator::Hydrator;

#[async_trait]
pub trait PulseEventSource: Send + Sync {
    async fn reactions_for_posts(
        &self,
        post_ids: &[u64],
        scoring_time_ms: u64,
    ) -> Result<Vec<Vec<ReactionEvent>>, String>;

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
        let author_ids: Vec<u64> = candidates.iter().map(|candidate| candidate.author_id).collect();

        let reactions = match self.source.reactions_for_posts(&post_ids, scoring_time_ms).await {
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
        let authors = match self.source.author_observations(&author_ids, scoring_time_ms).await {
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

        let mut excluded = query.user_features.blocked_user_ids.clone();
        excluded.extend(query.user_features.muted_user_ids.iter().copied());

        reactions
            .into_iter()
            .zip(authors)
            .map(|(events, author)| {
                let allowed = viewer_influence_mask(&events, &excluded);
                Ok(PostCandidate {
                    pulse: evidence_from_observations(
                        scoring_time_ms,
                        LAGGED_WINDOW_MS,
                        &allowed,
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
