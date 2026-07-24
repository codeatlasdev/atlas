use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

use crate::types::{Event, Timestamp};

const EVENTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("events");

pub fn append(db: &Database, event: &Event) -> atlas_core::Result<()> {
    let bytes = serde_json::to_vec(event)
        .map_err(atlas_core::AtlasError::Serialization)?;

    let txn = db.begin_write()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    {
        let mut table = txn.open_table(EVENTS_TABLE)
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        table.insert(event.id.as_str(), bytes.as_slice())
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    }
    txn.commit()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    Ok(())
}

pub fn list_since(db: &Database, since: Timestamp) -> atlas_core::Result<Vec<Event>> {
    let txn = db.begin_read()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    let table = txn.open_table(EVENTS_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    let mut events = Vec::new();
    let iter = table.iter()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;

    for entry in iter {
        let entry = entry
            .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
        let bytes = entry.1.value();
        let event: Event = serde_json::from_slice(bytes)
            .map_err(atlas_core::AtlasError::Serialization)?;
        if event.timestamp >= since {
            events.push(event);
        }
    }

    events.sort_by_key(|e| e.timestamp);
    Ok(events)
}

pub fn count(db: &Database) -> atlas_core::Result<u64> {
    let txn = db.begin_read()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    let table = txn.open_table(EVENTS_TABLE)
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    let len = table.len()
        .map_err(|e| atlas_core::AtlasError::Database(e.to_string()))?;
    Ok(len)
}
