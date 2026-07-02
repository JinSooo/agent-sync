use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredRecord {
    pub id: String,
    pub kind: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub json: String,
}

pub struct AgentSyncStore {
    conn: Connection,
}

impl AgentSyncStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    pub fn open_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    pub fn init(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_records_kind ON records(kind);",
        )
    }

    pub fn save_json<T: Serialize>(
        &self,
        kind: &str,
        id: Option<Uuid>,
        value: &T,
    ) -> rusqlite::Result<String> {
        let id = id.unwrap_or_else(Uuid::new_v4).to_string();
        let now = Utc::now().to_rfc3339();
        let json = serde_json::to_string(value)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        self.conn.execute(
            "INSERT INTO records (id, kind, created_at, updated_at, json)
             VALUES (?1, ?2, ?3, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET kind=excluded.kind, updated_at=excluded.updated_at, json=excluded.json",
            params![id, kind, now, json],
        )?;
        Ok(id)
    }

    pub fn list(&self, kind: &str) -> rusqlite::Result<Vec<StoredRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, created_at, updated_at, json FROM records WHERE kind = ?1 ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![kind], |row| {
            let created: String = row.get(2)?;
            let updated: String = row.get(3)?;
            Ok(StoredRecord {
                id: row.get(0)?,
                kind: row.get(1)?,
                created_at: DateTime::parse_from_rfc3339(&created)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                json: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn delete(&self, kind: &str, id: &str) -> rusqlite::Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM records WHERE kind = ?1 AND id = ?2",
            params![kind, id],
        )?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stores_and_lists_json_records() {
        let store = AgentSyncStore::open_memory().unwrap();
        let id = store
            .save_json("snapshot", None, &json!({"ok": true}))
            .unwrap();
        let rows = store.list("snapshot").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert!(store.delete("snapshot", &id).unwrap());
        assert!(store.list("snapshot").unwrap().is_empty());
        assert!(!store.delete("snapshot", &id).unwrap());
    }
}
