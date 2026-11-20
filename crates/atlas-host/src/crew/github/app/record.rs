//! What GitHub's conversion gave this server, kept in `<crew dir>/github-app.json`: owner-only (0600), written
//! atomically, never `secrets.json` (that file is a crewmate's and reaches its environment), never an env var and never
//! a reply: [`AppRecord::public`] is the only view that leaves this module.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const FILE: &str = "github-app.json";

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AppRecord {
    /// The numeric app id.
    pub id: u64,
    /// What the JWT's `iss` carries (GitHub's current rule), not the numeric id.
    pub client_id: String,
    pub slug: String,
    pub name: String,
    /// The login of the account that owns the app.
    pub owner: String,
    /// The private key, PEM (PKCS#1, as GitHub returns it).
    pub pem: String,
    pub webhook_secret: String,
    /// What the app was created with: a token never asks for more.
    #[serde(default)]
    pub permissions: Value,
    pub created_at: String,
    /// The id of `<slug>[bot]`, for its noreply email. Looked up once, after the conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<u64>,
}

impl std::fmt::Debug for AppRecord {
    // Never prints the key: a `{:?}` of this record in a log is not a leak.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppRecord")
            .field("id", &self.id)
            .field("slug", &self.slug)
            .finish_non_exhaustive()
    }
}

impl AppRecord {
    /// The commit identity of the app: `<slug>[bot]`, and the noreply email GitHub links to the bot's profile.
    pub(crate) fn identity(&self) -> Identity {
        let name = format!("{}[bot]", self.slug);
        let email = match self.bot_id {
            Some(id) => format!("{id}+{name}@users.noreply.github.com"),
            None => format!("{name}@users.noreply.github.com"),
        };
        Identity { name, email }
    }

    /// The fields a client may read.
    pub(crate) fn public(&self) -> Value {
        json!({
            "id": self.id,
            "slug": self.slug,
            "name": self.name,
            "owner": self.owner,
            "permissions": self.permissions,
            "created_at": self.created_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub name: String,
    pub email: String,
}

pub(crate) struct AppStore {
    path: PathBuf,
}

impl AppStore {
    pub(crate) fn new(crew_dir: &Path) -> Self {
        Self {
            path: crew_dir.join(FILE),
        }
    }

    /// The app, `None` when there is none. A file that cannot be read is an error, not "no app": the owner
    /// is told, and nothing quietly falls back to another login.
    pub(crate) fn load(&self) -> Result<Option<AppRecord>, String> {
        // tree-access: hostd's own file in the crew dir, never a crewmate's tree
        match std::fs::read(&self.path) {
            Ok(b) => serde_json::from_slice(&b)
                .map(Some)
                .map_err(|e| format!("{} is unreadable: {e}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", self.path.display())),
        }
    }

    pub(crate) fn save(&self, rec: &AppRecord) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(rec).map_err(|e| e.to_string())?;
        crate::paths::write_private(&self.path, &bytes)
            .map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// Delete the file. `false` when there was none.
    pub(crate) fn remove(&self) -> Result<bool, String> {
        // tree-access: hostd's own file in the crew dir, never a crewmate's tree
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("{}: {e}", self.path.display())),
        }
    }
}
