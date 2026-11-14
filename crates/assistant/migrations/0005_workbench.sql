CREATE TABLE runtime_skills (id text PRIMARY KEY,name text NOT NULL,description text NOT NULL DEFAULT '',source text NOT NULL,content text NOT NULL,enabled boolean NOT NULL DEFAULT true,builtin boolean NOT NULL DEFAULT false,deleted_at timestamptz,created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE knowledge_documents (id uuid PRIMARY KEY,title text NOT NULL,content text NOT NULL DEFAULT '',revision bigint NOT NULL DEFAULT 1,related_ids uuid[] NOT NULL DEFAULT '{}',deleted_at timestamptz,created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now());
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX knowledge_search ON knowledge_documents USING gin ((title||' '||content) gin_trgm_ops) WHERE deleted_at IS NULL;
CREATE TABLE harness_events (id uuid PRIMARY KEY,run_id uuid NOT NULL REFERENCES runs(id),message_id uuid NOT NULL REFERENCES messages(id),kind text NOT NULL,title text NOT NULL,content text NOT NULL DEFAULT '',status text NOT NULL DEFAULT 'running',result jsonb,created_at timestamptz NOT NULL DEFAULT now(),finished_at timestamptz);
CREATE INDEX harness_run_events ON harness_events(run_id,created_at,id);
