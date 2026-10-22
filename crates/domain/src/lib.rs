//! Domain model of *Family Year Planner*: types, rules and validation. No I/O.

pub mod birthday;
pub mod catalog;
pub mod dates;
pub mod free;
pub mod imported;
pub mod lang;
pub mod render;
pub mod types;
pub mod validation;

pub use catalog::{CatalogError, Labels, Names, TypeCatalog, TypeDef, TypeInfo};
pub use lang::Lang;
pub use types::{CalendarKind, Event, EventId, EventType, RenderKind, Source};
pub use validation::{EventInput, EventPatch, NewEvent, ValidationContext, ValidationError};
