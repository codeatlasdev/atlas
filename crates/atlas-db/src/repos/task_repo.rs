use sqlx::SqlitePool;

use atlas_core::domain::task::{Task, TaskPriority, TaskStatus};
use atlas_core::{AtlasError, Result};

use crate::models::TaskRow;

pub struct SqliteTaskRepo {
    pool: SqlitePool,
}

impl SqliteTaskRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_domain(row: TaskRow) -> Result<Task> {
        let labels: Vec<String> = serde_json::from_str(&row.labels).unwrap_or_default();

        Ok(Task {
            id: row.id,
            title: row.title,
            description: row.description,
            status: row.status.parse::<TaskStatus>()?,
            priority: row.priority.parse::<TaskPriority>()?,
            assigned_agent: row.assigned_agent,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            labels,
            branch: row.branch,
            pr_url: row.pr_url,
        })
    }

    pub async fn list_by_project(&self, project_path: &str) -> Result<Vec<Task>> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT * FROM tasks WHERE project_path = ? ORDER BY created_at DESC",
        )
        .bind(project_path)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtlasError::Database(e.to_string()))?;

        rows.into_iter().map(Self::row_to_domain).collect()
    }

    pub async fn create(&self, project_path: &str, task: &Task) -> Result<()> {
        let labels_json = serde_json::to_string(&task.labels).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            "INSERT INTO tasks (id, project_path, title, description, status, priority, assigned_agent, created_at, updated_at, labels, branch, pr_url) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&task.id)
        .bind(project_path)
        .bind(&task.title)
        .bind(&task.description)
        .bind(task.status.to_string())
        .bind(task.priority.to_string())
        .bind(&task.assigned_agent)
        .bind(task.created_at.to_rfc3339())
        .bind(task.updated_at.to_rfc3339())
        .bind(&labels_json)
        .bind(&task.branch)
        .bind(&task.pr_url)
        .execute(&self.pool)
        .await
        .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn update_status(&self, id: &str, status: TaskStatus) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query("UPDATE tasks SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status.to_string())
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn assign_agent(&self, id: &str, agent_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query("UPDATE tasks SET assigned_agent = ?, updated_at = ? WHERE id = ?")
            .bind(agent_id)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AtlasError::Database(e.to_string()))?;

        Ok(())
    }
}
