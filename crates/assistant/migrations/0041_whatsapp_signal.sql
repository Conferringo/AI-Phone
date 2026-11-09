CREATE TABLE whatsapp_signal (
    binding_id uuid NOT NULL,
    kind smallint NOT NULL,
    address text NOT NULL,
    value bytea NOT NULL,
    PRIMARY KEY (binding_id, kind, address),
    CHECK (kind <> 0 OR octet_length(value) = 32)
);

CREATE INDEX whatsapp_signal_user ON whatsapp_signal (
    binding_id, (split_part(split_part(address, '@', 1), ':', 1))
) WHERE kind IN (0, 1);

CREATE TABLE whatsapp_prekeys (
    binding_id uuid NOT NULL,
    signed boolean NOT NULL,
    key_id bigint NOT NULL CHECK (key_id BETWEEN 0 AND 4294967295),
    value bytea NOT NULL,
    uploaded boolean NOT NULL,
    PRIMARY KEY (binding_id, signed, key_id)
);

CREATE TABLE whatsapp_msg_secrets (
    binding_id uuid NOT NULL,
    chat text NOT NULL,
    sender text NOT NULL,
    msg_id text NOT NULL,
    secret bytea NOT NULL CHECK (octet_length(secret) = 32),
    expires_at bigint NOT NULL,
    message_ts bigint NOT NULL,
    PRIMARY KEY (binding_id, chat, sender, msg_id)
);

CREATE INDEX whatsapp_msg_secrets_expiry ON whatsapp_msg_secrets (binding_id, expires_at)
    WHERE expires_at <> 0;
