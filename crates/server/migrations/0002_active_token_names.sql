-- Token names are unique among active (non-revoked) tokens.
CREATE UNIQUE INDEX pat_tokens_active_name ON pat_tokens (name) WHERE revoked_at IS NULL;
