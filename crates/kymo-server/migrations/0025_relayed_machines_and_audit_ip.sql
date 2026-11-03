-- A machine reached through a lab host's VPN keeps its own port numbers: the player sees the
-- machine's private address and its real ports (80, 22), and the gateway translates to where the
-- lab host relays them. `guest_port` is the primary service port inside the machine.
ALTER TABLE instance ADD COLUMN guest_port INTEGER;

-- Who did it from where: the address is kept with every audit entry.
ALTER TABLE audit ADD COLUMN ip TEXT;
CREATE INDEX audit_action ON audit (action);
