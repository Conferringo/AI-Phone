ALTER TABLE desktop_observations DROP COLUMN sha256;
ALTER TABLE device_transfers RENAME COLUMN sha256 TO checksum;
UPDATE device_transfers SET checksum='sha256:'||checksum WHERE checksum<>'';
