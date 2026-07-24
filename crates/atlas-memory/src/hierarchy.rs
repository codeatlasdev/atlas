use crate::types::{Memory, Timestamp};

pub const STM_CAPACITY: usize = 50;
pub const HEAT_PROMOTION_THRESHOLD: f64 = 5.0;
pub const HEAT_EVICTION_THRESHOLD: f64 = 0.1;

#[derive(Debug, Clone, PartialEq)]
pub enum MemoryTier {
    ShortTerm,
    MidTerm,
    LongTerm,
}

/// Compute heat score based on access patterns and recency.
///
/// Formula: (ln(access_count + 1) + interaction_weight) * recency_factor
/// - access_count: how many times the memory was accessed
/// - interaction_length: duration of interactions (proxy for importance)
/// - recency: 0.0..1.0 decay factor from temporal module
pub fn compute_heat(access_count: u32, interaction_length: u32, recency: f64) -> f64 {
    let access_factor = (f64::from(access_count) + 1.0).ln();
    let interaction_weight = f64::from(interaction_length) * 0.01;
    (access_factor + interaction_weight) * recency
}

pub fn should_promote(heat: f64) -> bool {
    heat >= HEAT_PROMOTION_THRESHOLD
}

pub fn should_evict(heat: f64) -> bool {
    heat < HEAT_EVICTION_THRESHOLD
}

/// Classify a memory into a tier based on its heat and age.
///
/// - ShortTerm: recently created (< 1 hour) OR high heat
/// - LongTerm: high access count and sustained heat
/// - MidTerm: everything else
pub fn classify_tier(memory: &Memory) -> MemoryTier {
    let now = chrono::Utc::now().timestamp_millis();
    let age_hours = age_in_hours(memory.created_at, now);

    if age_hours < 1.0 || memory.heat > HEAT_PROMOTION_THRESHOLD * 2.0 {
        return MemoryTier::ShortTerm;
    }

    if memory.access_count >= 5 && memory.heat >= HEAT_PROMOTION_THRESHOLD {
        return MemoryTier::LongTerm;
    }

    MemoryTier::MidTerm
}

fn age_in_hours(created_at: Timestamp, now: Timestamp) -> f64 {
    let diff_ms = (now - created_at).max(0) as f64;
    diff_ms / (1000.0 * 60.0 * 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_increases_with_access() {
        let h1 = compute_heat(1, 0, 1.0);
        let h2 = compute_heat(10, 0, 1.0);
        assert!(h2 > h1);
    }

    #[test]
    fn heat_decreases_with_low_recency() {
        let h1 = compute_heat(5, 10, 1.0);
        let h2 = compute_heat(5, 10, 0.1);
        assert!(h1 > h2);
    }

    #[test]
    fn promotion_threshold() {
        assert!(should_promote(5.0));
        assert!(should_promote(10.0));
        assert!(!should_promote(4.9));
    }

    #[test]
    fn eviction_threshold() {
        assert!(should_evict(0.05));
        assert!(!should_evict(0.1));
        assert!(!should_evict(1.0));
    }
}
