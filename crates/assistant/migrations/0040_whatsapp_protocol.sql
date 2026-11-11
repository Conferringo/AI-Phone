CREATE TABLE whatsapp_sync_keys (
    binding_id uuid NOT NULL,
    key_id bytea NOT NULL,
    key_data bytea NOT NULL,
    fingerprint bytea NOT NULL,
    key_timestamp bigint NOT NULL,
    PRIMARY KEY (binding_id, key_id)
);

CREATE TABLE whatsapp_sync_versions (
    binding_id uuid NOT NULL,
    collection text NOT NULL,
    state jsonb NOT NULL,
    PRIMARY KEY (binding_id, collection)
);

CREATE TABLE whatsapp_mutation_macs (
    binding_id uuid NOT NULL,
    collection text NOT NULL,
    index_mac bytea NOT NULL,
    version bigint NOT NULL,
    value_mac bytea NOT NULL,
    PRIMARY KEY (binding_id, collection, index_mac)
);

CREATE TABLE whatsapp_sender_key_devices (
    binding_id uuid NOT NULL,
    group_jid text NOT NULL,
    device_jid text NOT NULL,
    has_key boolean NOT NULL,
    PRIMARY KEY (binding_id, group_jid, device_jid)
);

CREATE INDEX whatsapp_sender_key_devices_device_idx
    ON whatsapp_sender_key_devices (binding_id, device_jid);

CREATE TABLE whatsapp_lid_mappings (
    binding_id uuid NOT NULL,
    lid text NOT NULL,
    phone_number text NOT NULL,
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL,
    learning_source text NOT NULL,
    PRIMARY KEY (binding_id, lid)
);

CREATE INDEX whatsapp_lid_mappings_phone_idx
    ON whatsapp_lid_mappings (binding_id, phone_number, updated_at DESC);

CREATE TABLE whatsapp_base_keys (
    binding_id uuid NOT NULL,
    address text NOT NULL,
    message_id text NOT NULL,
    base_key bytea NOT NULL,
    PRIMARY KEY (binding_id, address, message_id)
);

CREATE TABLE whatsapp_device_registry (
    binding_id uuid NOT NULL,
    user_id text NOT NULL,
    record jsonb NOT NULL,
    PRIMARY KEY (binding_id, user_id)
);

CREATE TABLE whatsapp_group_metadata (
    binding_id uuid NOT NULL,
    group_jid text NOT NULL,
    metadata bytea NOT NULL,
    PRIMARY KEY (binding_id, group_jid)
);

CREATE TABLE whatsapp_tc_tokens (
    binding_id uuid NOT NULL,
    jid text NOT NULL,
    token bytea NOT NULL,
    token_timestamp bigint NOT NULL,
    sender_timestamp bigint,
    PRIMARY KEY (binding_id, jid)
);

CREATE TABLE whatsapp_sent_messages (
    binding_id uuid NOT NULL,
    chat_jid text NOT NULL,
    message_id text NOT NULL,
    payload bytea NOT NULL,
    stored_at bigint NOT NULL DEFAULT floor(extract(epoch FROM now()))::bigint,
    PRIMARY KEY (binding_id, chat_jid, message_id)
);

CREATE INDEX whatsapp_sent_messages_time_idx
    ON whatsapp_sent_messages (binding_id, stored_at);

CREATE TABLE whatsapp_pending_inbound (
    binding_id uuid NOT NULL,
    chat_jid text NOT NULL,
    sender_jid text NOT NULL,
    message_id text NOT NULL,
    message bytea NOT NULL,
    stored_at bigint NOT NULL DEFAULT floor(extract(epoch FROM now()))::bigint,
    PRIMARY KEY (binding_id, chat_jid, sender_jid, message_id)
);

CREATE INDEX whatsapp_pending_inbound_time_idx
    ON whatsapp_pending_inbound (binding_id, stored_at);
