use redb::{Database, ReadableTable, TableDefinition};

use crate::types::Memory;

const MEMORIES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("memories");

pub fn store(db: &Database, memory: &Memory) -> atlas_core::Result<()> {
    let bytes = serde_json::to_vec(memory)
        .map_err(atlas_core::AtlasError::Serialization)?;

    let txn = db.begin_write()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    {
        let mut table = txn.open_table(MEMORIES_TABLE)
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        table.insert(memory.id.as_str(), bytes.as_slice())
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    }
    txn.commit()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    Ok(())
}

pub fn get(db: &Database, id: &str) -> atlas_core::Result<Option<Memory>> {
    let txn = db.begin_read()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    let table = txn.open_table(MEMORIES_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    match table.get(id) {
        Ok(Some(value)) => {
            let memory: Memory = serde_json::from_slice(value.value())
                .map_err(atlas_core::AtlasError::Serialization)?;
            Ok(Some(memory))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(atlas_core::AtlasError::Database(e.to_string())),
    }
}

pub fn list_all(db: &Database) -> atlas_core::Result<Vec<Memory>> {
    let txn = db.begin_read()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    let table = txn.open_table(MEMORIES_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    let mut memories = Vec::new();
    let iter = table.iter()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    for entry in iter {
        let entry = entry
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        let bytes = entry.1.value();
        let memory: Memory = serde_json::from_slice(bytes)
            .map_err(atlas_core::AtlasError::Serialization)?;
        memories.push(memory);
    }

    Ok(memories)
}

pub fn update_heat(db: &Database, id: &str, new_heat: f64) -> atlas_core::Result<()> {
    let mut memory = get(db, id)?
        .ok_or_else(|| atlas_core::AtlasError::NotFound(format!("memory {id}")))?;

    memory.heat = new_heat;
    memory.access_count += 1;
    memory.last_accessed = chrono::Utc::now().timestamp_millis();

    store(db, &memory)
}

pub fn delete(db: &Database, id: &str) -> atlas_core::Result<()> {
    let txn = db.begin_write()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    {
        let mut table = txn.open_table(MEMORIES_TABLE)
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        table.remove(id)
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    }
    txn.commit()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    Ok(())
}
