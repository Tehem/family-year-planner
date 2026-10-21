//! Calendar stores for *Family Year Planner*: the `CalendarStore` trait, the in-memory `DemoStore`
//! and the Google Calendar REST `GoogleStore`.

pub mod demo;
pub mod google;
pub mod recurrence;
pub mod registry;
pub mod store;

pub use demo::DemoStore;
pub use google::{GoogleConfig, GoogleStore};
pub use registry::TypeRegistry;
pub use store::{CalendarStore, DeleteScope, ImportedEvent, StoreError, StoreResult};
