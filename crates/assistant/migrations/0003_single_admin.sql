CREATE TABLE admin_account (
  singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
  username text NOT NULL UNIQUE CHECK(username='admin'),
  password_hash text NOT NULL
);
CREATE TABLE admin_sessions (
  token_hash text PRIMARY KEY,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL DEFAULT now()+interval '12 hours'
);
