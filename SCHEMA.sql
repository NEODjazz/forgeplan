-- Design sketch for enterprise persistence. NOT used by the prototype.
-- Run only after converting into versioned migrations and integration tests.
CREATE TABLE organizations (
    id uuid PRIMARY KEY,
    name text NOT NULL
);
CREATE TABLE projects (
    organization_id uuid NOT NULL REFERENCES organizations(id),
    id uuid NOT NULL,
    name text NOT NULL,
    revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
    published_revision bigint,
    engine_version text NOT NULL,
    model jsonb NOT NULL,
    PRIMARY KEY (organization_id, id)
);
CREATE TABLE project_revisions (
    organization_id uuid NOT NULL,
    project_id uuid NOT NULL,
    revision bigint NOT NULL,
    engine_version text NOT NULL,
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    actor_id uuid NOT NULL,
    PRIMARY KEY (organization_id, project_id, revision),
    FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id)
);
CREATE TABLE command_receipts (
    organization_id uuid NOT NULL,
    project_id uuid NOT NULL,
    command_id uuid NOT NULL,
    request_digest text NOT NULL,
    result jsonb NOT NULL,
    revision bigint NOT NULL,
    PRIMARY KEY (organization_id, project_id, command_id),
    FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id)
);
CREATE TABLE outbox_events (
    organization_id uuid NOT NULL,
    event_id uuid NOT NULL,
    project_id uuid NOT NULL,
    revision bigint NOT NULL,
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    delivered_at timestamptz,
    PRIMARY KEY (organization_id, event_id),
    FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id)
);
CREATE INDEX outbox_pending ON outbox_events(created_at) WHERE delivered_at IS NULL;
-- Membership, project ACL, audit, resources, timesheets and attachments are pending.
-- No authz claim is made by this schema. JSONB is an initial snapshot choice;
-- normalize query-heavy resources/assignments once domain queries are stable.
