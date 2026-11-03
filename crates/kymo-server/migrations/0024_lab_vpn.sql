-- The VPN per lab host. Every lab host runs its own gateway with its own authority, so a
-- player's certificate for one host works nowhere else, and revoking it there touches only
-- that host. Host 0 is this server's own gateway, as before.
CREATE TABLE vpn_client_new (
    user_id     INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    lab_host_id INTEGER NOT NULL DEFAULT 0,
    idx         INTEGER NOT NULL,
    serial      TEXT NOT NULL,
    issued_at   INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    PRIMARY KEY (user_id, lab_host_id),
    UNIQUE (lab_host_id, idx)
);
INSERT INTO vpn_client_new (user_id, lab_host_id, idx, serial, issued_at, expires_at)
    SELECT user_id, 0, idx, serial, issued_at, expires_at FROM vpn_client;
DROP TABLE vpn_client;
ALTER TABLE vpn_client_new RENAME TO vpn_client;

ALTER TABLE vpn_revoked ADD COLUMN lab_host_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vpn_lab ADD COLUMN lab_host_id INTEGER NOT NULL DEFAULT 0;

-- A machine reached through a lab host's VPN is at the lab network's own address, not at the
-- slot the platform reserved for it.
ALTER TABLE instance ADD COLUMN lab_addr TEXT;

-- What each lab host last said about its gateway, so the admin pages can show it without calling
-- out: where players connect, and whether it refuses published ports.
ALTER TABLE lab_host ADD COLUMN vpn_host TEXT;
ALTER TABLE lab_host ADD COLUMN vpn_port INTEGER;
ALTER TABLE lab_host ADD COLUMN vpn_only INTEGER NOT NULL DEFAULT 0;
