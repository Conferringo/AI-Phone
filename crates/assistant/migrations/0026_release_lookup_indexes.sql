-- Preserve every row. Cover hot message lookups and referencing columns before
-- chat/agent history grows; indexes also keep FK validation from scanning children.
CREATE INDEX runs_assistant_message ON runs(assistant_message_id);
CREATE INDEX harness_message ON harness_events(message_id);
CREATE INDEX run_inputs_message ON run_inputs(message_id);
CREATE INDEX desktop_actions_run ON desktop_actions(run_id);
CREATE INDEX device_links_target ON device_links(target_device_id);
CREATE INDEX device_transfers_run ON device_transfers(run_id);
CREATE INDEX device_transfers_source ON device_transfers(source_device_id);
CREATE INDEX device_transfers_target ON device_transfers(target_device_id);
CREATE INDEX run_device_bindings_device ON run_device_bindings(device_id);
CREATE INDEX device_messages_link ON device_messages(link_id);
CREATE INDEX device_messages_source ON device_messages(source_device_id);
CREATE INDEX device_messages_target ON device_messages(target_device_id);
CREATE INDEX device_messages_source_run ON device_messages(source_run_id);
CREATE INDEX device_messages_target_run ON device_messages(target_run_id);
CREATE INDEX app_learning_event ON app_learning_observations(event_id);
