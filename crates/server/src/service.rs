//! Event operations shared by the web API and the MCP server.

use annee_domain::validation::{validate_new, validate_update};
use annee_domain::{Event, EventId, EventInput, EventPatch, ValidationContext};
use annee_gcal::DeleteScope;

use crate::error::ServiceError;
use crate::state::AppState;

impl AppState {
    pub fn validation_context<'a>(&'a self, members: &'a [String]) -> ValidationContext<'a> {
        ValidationContext {
            members,
            today: self.today(),
        }
    }

    pub async fn events_for_year(&self, year: i32) -> Result<Vec<Event>, ServiceError> {
        Ok(self.store.list_year(year).await?)
    }

    pub async fn create_event(
        &self,
        input: &EventInput,
        writer: &str,
    ) -> Result<Event, ServiceError> {
        let members = &self.config.family.members;
        let new = validate_new(input, &self.validation_context(members))?;
        Ok(self.store.create(&new, writer).await?)
    }

    pub async fn update_event(
        &self,
        id: &str,
        patch: &EventPatch,
        writer: &str,
    ) -> Result<Event, ServiceError> {
        let id: EventId = id.parse().map_err(|_| ServiceError::BadId)?;
        let existing = self.store.get(&id).await?;
        let members = &self.config.family.members;
        let new = validate_update(&existing, patch, &self.validation_context(members))?;
        Ok(self.store.update(&id, &new, writer).await?)
    }

    pub async fn delete_event(&self, id: &str) -> Result<(EventId, DeleteScope), ServiceError> {
        let id: EventId = id.parse().map_err(|_| ServiceError::BadId)?;
        let existing = self.store.get(&id).await?;
        if existing.read_only {
            return Err(annee_domain::ValidationError::ReadOnly.into());
        }
        let scope = self.store.delete(&id).await?;
        Ok((id, scope))
    }
}
