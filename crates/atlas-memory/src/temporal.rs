use crate::types::{Memory, Timestamp};

/// Return memories that were valid at a given point in time.
/// A memory is valid at `timestamp` if:
/// - valid_from <= timestamp
/// - valid_until is None (still valid) OR valid_until > timestamp
pub fn valid_at(memories: &[Memory], timestamp: Timestamp) -> Vec<&Memory> {
    memories
        .iter()
        .filter(|m| {
            m.valid_from <= timestamp
                && m.valid_until.map_or(true, |until| until > timestamp)
        })
        .collect()
}

/// Return memories created between `from` and `to` (inclusive).
pub fn created_between(memories: &[Memory], from: Timestamp, to: Timestamp) -> Vec<&Memory> {
    memories
        .iter()
        .filter(|m| m.created_at >= from && m.created_at <= to)
        .collect()
}

/// Compute recency decay using a logarithmic decay curve.
///
/// Based on simplified Ebbinghaus forgetting curve:
/// retention = 1.0 / (1.0 + (hours_elapsed ^ 0.5))
///
/// Returns a value between 0.0 and 1.0 where:
/// - 1.0 = just accessed
/// - approaching 0.0 = very old
pub fn compute_recency_decay(last_accessed: Timestamp, now: Timestamp) -> f64 {
    let elapsed_ms = (now - last_accessed).max(0) as f64;
    let elapsed_hours = elapsed_ms / (1000.0 * 60.0 * 60.0);

    if elapsed_hours <= 0.0 {
        return 1.0;
    }

    1.0 / (1.0 + elapsed_hours.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::types::{MemorySource, MemoryType};

    fn make_memory(id: &str, valid_from: Timestamp, valid_until: Option<Timestamp>, created_at: Timestamp) -> Memory {
        Memory {
            id: id.to_string(),
            content: String::new(),
            memory_type: MemoryType::Fact,
            source: MemorySource {
                session_id: None,
                agent_name: None,
                project_path: None,
            },
            embedding: None,
            metadata: HashMap::new(),
            valid_from,
            valid_until,
            created_at,
            heat: 1.0,
            access_count: 0,
            last_accessed: created_at,
        }
    }

    #[test]
    fn valid_at_filters_correctly() {
        let memories = vec![
            make_memory("a", 100, None, 100),
            make_memory("b", 200, Some(300), 200),
            make_memory("c", 400, None, 400),
        ];

        let result = valid_at(&memories, 250);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "a");
        assert_eq!(result[1].id, "b");
    }

    #[test]
    fn created_between_filters_correctly() {
        let memories = vec![
            make_memory("a", 100, None, 100),
            make_memory("b", 200, None, 200),
            make_memory("c", 400, None, 400),
        ];

        let result = created_between(&memories, 150, 300);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "b");
    }

    #[test]
    fn recency_decay_is_one_for_just_accessed() {
        let decay = compute_recency_decay(1000, 1000);
        assert!((decay - 1.0).abs() < 1e-6);
    }

    #[test]
    fn recency_decay_decreases_over_time() {
        let one_hour = 1000 * 60 * 60;
        let d1 = compute_recency_decay(0, one_hour);
        let d2 = compute_recency_decay(0, one_hour * 24);
        assert!(d1 > d2);
        assert!(d1 > 0.0);
        assert!(d2 > 0.0);
    }
}
