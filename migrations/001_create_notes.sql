-- Notes: the records the MCP tools create, read, list, search and delete.
CREATE TABLE notes (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    title      TEXT        NOT NULL CHECK (length(btrim(title)) > 0 AND length(title) <= 200),
    content    TEXT        NOT NULL DEFAULT '' CHECK (length(content) <= 20000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Full-text search document, kept in sync by PostgreSQL.
    search     TSVECTOR    GENERATED ALWAYS AS (to_tsvector('english', title || ' ' || content)) STORED
);

-- list_notes: newest first, with the id as tiebreak so pages never shift.
CREATE INDEX notes_created_at_id_idx ON notes (created_at DESC, id DESC);

-- search_notes: full-text matching over title and content.
CREATE INDEX notes_search_idx ON notes USING GIN (search);
