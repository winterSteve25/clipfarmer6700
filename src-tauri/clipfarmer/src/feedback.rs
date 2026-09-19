use crate::domain::{ChannelProfile, Outcome, OutcomeMetrics};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait OutcomeCollector: Send + Sync {
    fn platform(&self) -> &str;
    async fn collect(&self, post: &Outcome, age_hours: u32) -> Result<Option<OutcomeMetrics>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedOutcome {
    pub retention_lift: Option<f64>,
    pub share_rate: f64,
    pub engagement_rate: f64,
}

pub fn normalize(metrics: &OutcomeMetrics, profile: &ChannelProfile) -> NormalizedOutcome {
    let views = metrics.views.max(1) as f64;
    let share_rate = metrics.shares as f64 / views;
    let engagement_rate =
        (metrics.likes + metrics.comments + metrics.shares + metrics.saves) as f64 / views;
    let retention_lift = match (metrics.completion_rate, profile.retention_baseline) {
        (Some(value), Some(baseline)) if baseline > 0.0 => Some(value / baseline - 1.0),
        _ => None,
    };
    NormalizedOutcome {
        retention_lift,
        share_rate,
        engagement_rate,
    }
}

pub fn update_profile(profile: &mut ChannelProfile, observations: &[OutcomeMetrics]) {
    if observations.is_empty() {
        return;
    }
    let completions = observations
        .iter()
        .filter_map(|metrics| metrics.completion_rate)
        .collect::<Vec<_>>();
    if !completions.is_empty() {
        profile.retention_baseline =
            Some(completions.iter().sum::<f64>() / completions.len() as f64);
    }
    let total_views = observations
        .iter()
        .map(|metrics| metrics.views)
        .sum::<u64>()
        .max(1);
    let total_shares = observations
        .iter()
        .map(|metrics| metrics.shares)
        .sum::<u64>();
    profile.share_rate_baseline = Some(total_shares as f64 / total_views as f64);
    profile.version = profile.version.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_uses_rates_instead_of_raw_views() {
        let mut profile = ChannelProfile::empty("channel");
        profile.retention_baseline = Some(0.5);
        let metrics = OutcomeMetrics {
            platform: "youtube".to_owned(),
            remote_id: "one".to_owned(),
            age_hours: 24,
            views: 100,
            average_watch_seconds: Some(20.0),
            completion_rate: Some(0.75),
            viewed_vs_swiped_away: Some(0.8),
            rewatches: Some(15),
            shares: 10,
            saves: 5,
            likes: 20,
            comments: 5,
        };
        let normalized = normalize(&metrics, &profile);
        assert_eq!(normalized.retention_lift, Some(0.5));
        assert_eq!(normalized.share_rate, 0.1);
        assert_eq!(normalized.engagement_rate, 0.4);
    }
}
