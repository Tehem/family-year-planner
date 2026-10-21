//! Event operations shared by the web API and the MCP server.

use annee_domain::imported::localize_all;
use annee_domain::validation::{validate_new, validate_update};
use annee_domain::{
    Event, EventId, EventInput, EventPatch, TypeCatalog, ValidationContext, ValidationError,
};
use annee_gcal::{DeleteScope, WriteAccess};

use crate::error::ServiceError;
use crate::i18n;
use crate::state::AppState;

impl AppState {
    pub fn validation_context<'a>(
        &'a self,
        members: &'a [String],
        catalog: &'a TypeCatalog,
    ) -> ValidationContext<'a> {
        ValidationContext {
            members,
            today: self.today(),
            catalog,
        }
    }

    /// Writes wait while the colors of the types are being migrated (§19.5). The guard is held
    /// for the whole write, so that a migration starts only once the writes in flight are done.
    fn write_access(&self) -> Result<WriteAccess, ServiceError> {
        self.types.try_write_access().ok_or(ServiceError::Migrating)
    }

    pub async fn events_for_year(&self, year: i32) -> Result<Vec<Event>, ServiceError> {
        let mut events = self.store.list_year(year).await?;
        localize_all(&mut events, i18n::current());
        Ok(events)
    }

    pub async fn create_event(
        &self,
        input: &EventInput,
        writer: &str,
    ) -> Result<Event, ServiceError> {
        let _access = self.write_access()?;
        let members = &self.config.family.members;
        let catalog = self.types.current();
        let new = validate_new(input, &self.validation_context(members, &catalog))?;
        Ok(self.store.create(&new, writer).await?)
    }

    pub async fn update_event(
        &self,
        id: &str,
        patch: &EventPatch,
        writer: &str,
    ) -> Result<Event, ServiceError> {
        let _access = self.write_access()?;
        let id: EventId = id.parse().map_err(|_| ServiceError::BadId)?;
        let existing = self.store.get(&id).await?;
        let members = &self.config.family.members;
        let catalog = self.types.current();
        let new = validate_update(
            &existing,
            patch,
            &self.validation_context(members, &catalog),
        )?;
        Ok(self.store.update(&id, &new, writer).await?)
    }

    pub async fn delete_event(&self, id: &str) -> Result<(EventId, DeleteScope), ServiceError> {
        let _access = self.write_access()?;
        let id: EventId = id.parse().map_err(|_| ServiceError::BadId)?;
        let existing = self.store.get(&id).await?;
        if existing.read_only {
            return Err(ValidationError::ReadOnly.into());
        }
        let scope = self.store.delete(&id).await?;
        Ok((id, scope))
    }
}
