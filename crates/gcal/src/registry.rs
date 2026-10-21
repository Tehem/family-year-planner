//! The live type catalog shared by the server and the calendar stores (§19.6).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use annee_domain::TypeCatalog;

/// Holds the effective [`TypeCatalog`]; swapped atomically when the family changes the types.
#[derive(Debug)]
pub struct TypeRegistry {
    catalog: RwLock<Arc<TypeCatalog>>,
    migrating: AtomicBool,
}

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
        }
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
