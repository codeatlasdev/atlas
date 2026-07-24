use std::collections::{HashSet, VecDeque};

use redb::{Database, ReadableTable, TableDefinition};

use crate::types::{Entity, Relationship};

const ENTITIES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("entities");
const RELATIONSHIPS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("relationships");

pub fn add_entity(db: &Database, entity: &Entity) -> atlas_core::Result<()> {
    let bytes = serde_json::to_vec(entity)
        .map_err(atlas_core::AtlasError::Serialization)?;

    let txn = db.begin_write()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    {
        let mut table = txn.open_table(ENTITIES_TABLE)
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        table.insert(entity.id.as_str(), bytes.as_slice())
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    }
    txn.commit()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    Ok(())
}

pub fn add_relationship(db: &Database, rel: &Relationship) -> atlas_core::Result<()> {
    let bytes = serde_json::to_vec(rel)
        .map_err(atlas_core::AtlasError::Serialization)?;

    let txn = db.begin_write()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    {
        let mut table = txn.open_table(RELATIONSHIPS_TABLE)
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        table.insert(rel.id.as_str(), bytes.as_slice())
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    }
    txn.commit()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    Ok(())
}

pub fn get_entity(db: &Database, id: &str) -> atlas_core::Result<Option<Entity>> {
    let txn = db.begin_read()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    let table = txn.open_table(ENTITIES_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    match table.get(id) {
        Ok(Some(value)) => {
            let entity: Entity = serde_json::from_slice(value.value())
                .map_err(atlas_core::AtlasError::Serialization)?;
            Ok(Some(entity))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(atlas_core::AtlasError::Database(e.to_string())),
    }
}

/// Get all relationships where entity_id is either source or target,
/// along with the connected entity on the other side.
pub fn get_neighbors(db: &Database, entity_id: &str) -> atlas_core::Result<Vec<(Relationship, Entity)>> {
    let txn = db.begin_read()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    let rel_table = txn.open_table(RELATIONSHIPS_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    let ent_table = txn.open_table(ENTITIES_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    let mut results = Vec::new();

    let iter = rel_table.iter()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    for entry in iter {
        let entry = entry
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        let bytes = entry.1.value();
        let rel: Relationship = serde_json::from_slice(bytes)
            .map_err(atlas_core::AtlasError::Serialization)?;

        let neighbor_id = if rel.source == entity_id {
            Some(&rel.target)
        } else if rel.target == entity_id {
            Some(&rel.source)
        } else {
            None
        };

        if let Some(nid) = neighbor_id {
            if let Ok(Some(value)) = ent_table.get(nid.as_str()) {
                let entity: Entity = serde_json::from_slice(value.value())
                    .map_err(atlas_core::AtlasError::Serialization)?;
                results.push((rel, entity));
            }
        }
    }

    Ok(results)
}

/// Breadth-first search from a starting entity, up to max_depth hops.
/// Returns all reachable entities (excluding the start node).
pub fn bfs(db: &Database, start: &str, max_depth: u32) -> atlas_core::Result<Vec<Entity>> {
    let txn = db.begin_read()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    let rel_table = txn.open_table(RELATIONSHIPS_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    let ent_table = txn.open_table(ENTITIES_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    // Load all relationships into memory for efficient traversal
    let mut all_rels: Vec<Relationship> = Vec::new();
    let iter = rel_table.iter()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    for entry in iter {
        let entry = entry
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        let rel: Relationship = serde_json::from_slice(entry.1.value())
            .map_err(atlas_core::AtlasError::Serialization)?;
        all_rels.push(rel);
    }

    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(start.to_string());

    let mut queue: VecDeque<(String, u32)> = VecDeque::new();
    queue.push_back((start.to_string(), 0));

    let mut results: Vec<Entity> = Vec::new();

    while let Some((current_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        // Find neighbors via relationships
        for rel in &all_rels {
            let neighbor_id = if rel.source == current_id {
                Some(&rel.target)
            } else if rel.target == current_id {
                Some(&rel.source)
            } else {
                None
            };

            if let Some(nid) = neighbor_id {
                if !visited.contains(nid.as_str()) {
                    visited.insert(nid.clone());
                    queue.push_back((nid.clone(), depth + 1));

                    if let Ok(Some(value)) = ent_table.get(nid.as_str()) {
                        if let Ok(entity) = serde_json::from_slice::<Entity>(value.value()) {
                            results.push(entity);
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}
