ALTER TABLE proactive_state ADD COLUMN wechat_watch boolean NOT NULL DEFAULT false;
ALTER TABLE proactive_state ADD COLUMN attention_connected boolean NOT NULL DEFAULT false;
ALTER TABLE proactive_state ADD COLUMN wechat_quiet_until timestamptz NOT NULL DEFAULT now();
