//! Domain model of *Family Year Planner*: types, rules and validation. No I/O.

pub mod birthday;
pub mod dates;
pub mod free;
pub mod render;
pub mod types;
pub mod validation;

pub use types::{CalendarKind, Event, EventId, EventType, RenderKind, Source, TypeInfo};
pub use validation::{EventInput, EventPatch, NewEvent, ValidationContext, ValidationError};
