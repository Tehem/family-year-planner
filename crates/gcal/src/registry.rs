//! The live type catalog shared by the server and the calendar stores (§19.6).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock as AsyncRwLock};

use annee_domain::TypeCatalog;

/// Holds the effective [`TypeCatalog`]; swapped atomically when the family changes the types.
#[derive(Debug)]
pub struct TypeRegistry {
    catalog: RwLock<Arc<TypeCatalog>>,
    migrating: AtomicBool,
    /// Writers (user writes, import runs) hold it shared; a color migration holds it exclusively,
    /// so that it starts only once the writes already in flight are done (§19.5).
    gate: Arc<AsyncRwLock<()>>,
}

/// Held by a write (or an import run) for as long as it can write events.
pub type WriteAccess = OwnedRwLockReadGuard<()>;
/// Held by a color migration while it rewrites events.
pub type MigrationAccess = OwnedRwLockWriteGuard<()>;

impl Default for TypeRegistry {
    fn default() -> Self {
        TypeRegistry::new(TypeCatalog::default())
    }
}

impl TypeRegistry {
    pub fn new(catalog: TypeCatalog) -> Self {
        TypeRegistry {
            catalog: RwLock::new(Arc::new(catalog)),
            migrating: AtomicBool::new(false),
            gate: Arc::new(AsyncRwLock::new(())),
        }
    }

    /// Permission to write events, or `None` while a color migration runs (or is suspended).
    pub fn try_write_access(&self) -> Option<WriteAccess> {
        if self.is_migrating() {
            return None;
        }
        let guard = self.gate.clone().try_read_owned().ok()?;
        // The flag may have flipped between the check and the lock.
        (!self.is_migrating()).then_some(guard)
    }

    /// Block new writes, then wait for the ones in flight. Hold the result for the whole pass.
    pub async fn begin_migration(&self) -> MigrationAccess {
        self.set_migrating(true);
        self.gate.clone().write_owned().await
    }

    /// The current catalog (cheap clone of an `Arc`).
    pub fn current(&self) -> Arc<TypeCatalog> {
        self.catalog.read().unwrap().clone()
    }

    pub fn set(&self, catalog: TypeCatalog) {
        *self.catalog.write().unwrap() = Arc::new(catalog);
    }

    /// Whether a color migration (§19.5) is running: writes and imports must wait.
    pub fn is_migrating(&self) -> bool {
        self.migrating.load(Ordering::SeqCst)
    }

    pub fn set_migrating(&self, migrating: bool) {
        self.migrating.store(migrating, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_migration_waits_for_writes_in_flight_and_blocks_new_ones() {
        let r = Arc::new(TypeRegistry::default());
        let write = r.try_write_access().expect("writes are allowed");
        let r2 = r.clone();
        let migration = tokio::spawn(async move { r2.begin_migration().await });
        // New writes are refused as soon as the migration is announced…
        while !r.is_migrating() {
            tokio::task::yield_now().await;
        }
        assert!(r.try_write_access().is_none());
        // …but it only starts once the write in flight is over.
        assert!(!migration.is_finished());
        drop(write);
        let held = migration.await.unwrap();
        assert!(r.try_write_access().is_none());
        drop(held);
        // Still suspended until the flag is cleared (a failed migration keeps writes blocked).
        assert!(r.try_write_access().is_none());
        r.set_migrating(false);
        assert!(r.try_write_access().is_some());
    }
}
