use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct ServerRow {
    pub id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: i64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct ServiceRow {
    pub id: String,
    pub server_id: String,
    pub name: String,
    pub unit_name: String,
    pub state: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, FromRow)]
pub struct SessionRow {
    pub id: String,
    pub kind: String,
    pub server_id: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub metadata: String,
}

#[derive(Debug, FromRow)]
pub struct ConfigRow {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}
