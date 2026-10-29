ALTER TABLE device_runtimes DROP CONSTRAINT device_runtimes_provider_check;
ALTER TABLE device_runtimes ADD CONSTRAINT device_runtimes_provider_check CHECK(provider IN ('docker-desktop','android-emulator'));
CREATE TABLE phone_provision_jobs (
 id uuid PRIMARY KEY,
 device_id text NOT NULL REFERENCES devices(id),
 status text NOT NULL CHECK(status IN ('queued','running','completed','failed')),
 stage text NOT NULL DEFAULT '检查系统镜像与空间',
 error text NOT NULL DEFAULT '',
 template_id text NOT NULL CHECK(template_id='android35-arm64'),
 cpus integer NOT NULL CHECK(cpus IN (1,2,4)),
 memory_mb integer NOT NULL CHECK(memory_mb IN (1536,2048,3072,4096)),
 storage_gb integer NOT NULL CHECK(storage_gb IN (16,24,32)),
 identity_profile text NOT NULL CHECK(identity_profile IN ('aiphone','xiaomi8')),
 created_at timestamptz NOT NULL DEFAULT now(),
 updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX phone_provision_active ON phone_provision_jobs(device_id) WHERE status IN ('queued','running');
