-- One row per daily "still open" digest, keyed by local (Eastern) date, so a
-- restart never sends the same day's digest twice.
CREATE TABLE IF NOT EXISTS digests (
    digest_date TEXT PRIMARY KEY,
    sent_at TEXT NOT NULL,
    open_incidents INTEGER NOT NULL
);
