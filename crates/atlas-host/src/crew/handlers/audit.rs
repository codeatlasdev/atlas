//! `crewmate_audit` / `crew_audit`: read the append-only audit logs (`crew::audit`).

use serde_json::{Value, json};

use super::super::audit::Query;
use crate::server::Hub;

/// `CrewmateAudit` (`Some(id)`) and `CrewAudit` (`None`): newest first, filtered by time and kind.
pub(crate) fn read(
    hub: &Hub,
    id: Option<String>,
    since: Option<u64>,
    limit: usize,
    kind: Option<String>,
) -> Result<Value, String> {
    // A crewmate's log outlives it only through the crew-level entries, so an unknown id is an error
    // (and a path like `../x` never reaches the file system).
    if let Some(id) = &id
        && hub.crew.store.load_crewmate(id).is_err()
    {
        return Err(format!("crewmate {id} not found"));
    }
    let entries = hub
        .crew
        .audit
        .read(id.as_deref(), &Query { since, limit, kind });
    Ok(json!({ "entries": entries }))
}
