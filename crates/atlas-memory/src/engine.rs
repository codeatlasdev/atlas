use std::path::Path;

use redb::Database;

use crate::types::*;
use crate::{event, graph, hierarchy, store, temporal, vector};

pub struct MemoryEngine {
    db: Database,
    stm_buffer: Vec<Memory>,
}

impl MemoryEngine {
    pub fn open(path: &Path) -> atlas_core::Result<Self> {
        let db = Database::create(path)
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

        Ok(Self {
            db,
            stm_buffer: Vec::with_capacity(hierarchy::STM_CAPACITY),
        })
    }

    pub fn close(self) -> atlas_core::Result<()> {
        // redb Database drops cleanly on its own; explicit drop for clarity
        drop(self.db);
        Ok(())
    }

    // ─── Core Operations ───────────────────────────────────────────────

    pub fn store_memory(&mut self, memory: Memory) -> atlas_core::Result<MemoryId> {
        let id = memory.id.clone();

        // Persist to redb
        store::store(&self.db, &memory)?;

        // Add to STM buffer (ring buffer behavior)
        if self.stm_buffer.len() >= hierarchy::STM_CAPACITY {
            self.stm_buffer.remove(0);
        }
        self.stm_buffer.push(memory);

        Ok(id)
    }

    pub fn get_memory(&self, id: &str) -> atlas_core::Result<Option<Memory>> {
        // Check STM buffer first (hot cache)
        if let Some(mem) = self.stm_buffer.iter().find(|m| m.id == id) {
            return Ok(Some(mem.clone()));
        }
        store::get(&self.db, id)
    }

    pub fn search(&self, query: SearchQuery) -> atlas_core::Result<Vec<SearchResult>> {
        let all_memories = store::list_all(&self.db)?;
        let mut results: Vec<SearchResult> = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();

        // Vector search
        if let Some(ref query_embedding) = query.embedding {
            let similar = vector::find_similar(&all_memories, query_embedding, query.limit);
            for (idx, score) in similar {
                results.push(SearchResult {
                    memory: all_memories[idx].clone(),
                    score: f64::from(score),
                    source: SearchSource::Vector,
                });
            }
        }

        // Keyword search (simple substring matching)
        if let Some(ref text) = query.text {
            let lower_text = text.to_lowercase();
            for mem in &all_memories {
                if mem.content.to_lowercase().contains(&lower_text) {
                    // Avoid duplicates from vector search
                    if !results.iter().any(|r| r.memory.id == mem.id) {
                        results.push(SearchResult {
                            memory: mem.clone(),
                            score: 0.5, // base keyword match score
                            source: SearchSource::Keyword,
                        });
                    }
                }
            }
        }

        // Filter by memory types
        if let Some(ref types) = query.memory_types {
            results.retain(|r| types.contains(&r.memory.memory_type));
        }

        // Filter by time range
        if let Some((from, to)) = query.time_range {
            results.retain(|r| {
                let valid = temporal::valid_at(std::slice::from_ref(&r.memory), from);
                !valid.is_empty() && r.memory.created_at <= to
            });
        }

        // Apply temporal boost (recency)
        for result in &mut results {
            let recency = temporal::compute_recency_decay(result.memory.last_accessed, now);
            result.score *= recency;
        }

        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(query.limit);

        Ok(results)
    }

    // ─── Event Log ─────────────────────────────────────────────────────

    pub fn append_event(&self, event: Event) -> atlas_core::Result<()> {
        event::append(&self.db, &event)
    }

    pub fn get_events(&self, since: Timestamp) -> atlas_core::Result<Vec<Event>> {
        event::list_since(&self.db, since)
    }

    // ─── Graph ─────────────────────────────────────────────────────────

    pub fn add_entity(&self, entity: Entity) -> atlas_core::Result<()> {
        graph::add_entity(&self.db, &entity)
    }

    pub fn add_relationship(&self, rel: Relationship) -> atlas_core::Result<()> {
        graph::add_relationship(&self.db, &rel)
    }

    pub fn get_related(&self, entity_id: &str, depth: u32) -> atlas_core::Result<Vec<Entity>> {
        graph::bfs(&self.db, entity_id, depth)
    }

    // ─── Lifecycle ─────────────────────────────────────────────────────

    /// Called periodically to decay heat, promote/evict memories.
    pub fn tick(&mut self) -> atlas_core::Result<()> {
        let all_memories = store::list_all(&self.db)?;
        let now = chrono::Utc::now().timestamp_millis();

        for memory in &all_memories {
            let recency = temporal::compute_recency_decay(memory.last_accessed, now);
            let new_heat = hierarchy::compute_heat(memory.access_count, 1, recency);

            if hierarchy::should_evict(new_heat) {
                tracing::debug!(id = %memory.id, heat = new_heat, "evicting cold memory");
                store::delete(&self.db, &memory.id)?;
            } else if (new_heat - memory.heat).abs() > 0.01 {
                store::update_heat(&self.db, &memory.id, new_heat)?;
            }
        }

        Ok(())
    }
}
