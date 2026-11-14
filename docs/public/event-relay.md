# Event relay

Redis wakes connected-app delivery as soon as a task result commits. The worker reads the result from PostgreSQL. It checks every three seconds while subscribed, and every 300 ms if disconnected. Local chat and device execution use in-process signals.

Device handoffs wake their scheduler after PostgreSQL commits. To export new handoffs for an external workflow, start Kafka and restart the source deployment with its broker address:

```sh
docker compose -p ai-phone-events -f compose.kafka.yaml --profile events up -d --wait kafka
docker compose -p ai-phone-events -f compose.kafka.yaml --profile events exec kafka /opt/kafka/bin/kafka-topics.sh \
  --bootstrap-server kafka:9092 --create --if-not-exists \
  --topic ai-phone.device-handoff.accepted.v1 --partitions 1 --replication-factor 1
AI_PHONE_KAFKA_BROKER=127.0.0.1:59092 ./scripts/phone-lab-start.sh
```

Topic `ai-phone.device-handoff.accepted.v1` carries protobuf [`DeviceMessage`](../../packages/protocol/proto/ai_phone/v1/assistant.proto) events with the message ID, source and target device IDs, and acceptance time. Use the message ID as the deduplication key. Kafka acknowledgements are recorded in PostgreSQL; a broker outage leaves events pending for the next connection. Consumers can read from their own offsets.

---

Redis 在结果提交后唤醒连接应用的回复进程，回复内容仍从 PostgreSQL 读取。手机和电脑交接提交后立即调度。需要在外部工作流订阅、重放设备交接回执时，按上面的命令启用 Kafka。桌面安装默认使用随包 Redis；Kafka 是源码部署的可选服务。
