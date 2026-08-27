ALTER TABLE model_configs ADD COLUMN codex_outbound_enabled INTEGER NOT NULL DEFAULT 0 CHECK(codex_outbound_enabled IN (0, 1));
