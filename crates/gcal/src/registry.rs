//! The live type catalog shared by the server and the calendar stores (§19.6).

use std::sync::{Arc, RwLock};

use annee_domain::TypeCatalog;

/// Holds the effective [`TypeCatalog`]; swapped atomically when the family changes the types.
#[derive(Debug)]
pub struct TypeRegistry(RwLock<Arc<TypeCatalog>>);

impl Default for TypeRegistry {
    fn default() -> Self {
        TypeRegistry::new(TypeCatalog::default())
    }
}

impl TypeRegistry {
    pub fn new(catalog: TypeCatalog) -> Self {
        TypeRegistry(RwLock::new(Arc::new(catalog)))
    }

    /// The current catalog (cheap clone of an `Arc`).
    pub fn current(&self) -> Arc<TypeCatalog> {
        self.0.read().unwrap().clone()
    }

    pub fn set(&self, catalog: TypeCatalog) {
        *self.0.write().unwrap() = Arc::new(catalog);
    }
}
