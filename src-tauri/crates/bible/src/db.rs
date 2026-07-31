use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OpenFlags};

use crate::error::BibleError;

pub struct BibleDb {
    pub(crate) conn: Mutex<Connection>,
}

impl std::fmt::Debug for BibleDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BibleDb").finish_non_exhaustive()
    }
}

impl BibleDb {
    pub(crate) fn conn(&self) -> Result<MutexGuard<'_, Connection>, BibleError> {
        self.conn
            .lock()
            .map_err(|_| BibleError::Internal("Bible database lock was poisoned".to_string()))
    }

    /// Wrap an already-open connection. Exists so integration tests can build
    /// a small in-memory corpus without touching the real database file.
    #[must_use]
    pub fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    pub fn open(path: &Path) -> Result<Self, BibleError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_readonly(path: &Path) -> Result<Self, BibleError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )?;
        conn.execute_batch("PRAGMA query_only = ON;")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::BibleDb;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn open_readonly_rejects_missing_file() {
        let missing = Path::new("definitely-missing-bible-db-for-tests.sqlite");
        let err = BibleDb::open_readonly(missing).unwrap_err();
        assert!(
            err.to_string().contains("no such file") || err.to_string().contains("unable to open")
        );
    }

    #[test]
    fn open_in_memory_database() {
        let db = BibleDb::open(Path::new(":memory:")).expect("open");
        let conn = db.conn().expect("conn");
        let count: i64 = conn
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("query");
        assert_eq!(count, 1);
    }

    #[test]
    fn conn_reports_poisoned_lock_error() {
        let db = Arc::new(BibleDb::open(Path::new(":memory:")).expect("open"));
        let poison_target = Arc::clone(&db);
        let handle = std::thread::spawn(move || {
            let _guard = poison_target.conn.lock().unwrap();
            panic!("intentional poison");
        });
        assert!(handle.join().is_err());
        let err = db.conn().expect_err("poisoned lock");
        assert!(err.to_string().contains("poisoned"));
    }
}
