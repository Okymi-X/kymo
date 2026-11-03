-- A machine is reached at its own address with every port open on it, so players are given just
-- the address and scan it (HackTheBox machine style); a service is given as address and port.
ALTER TABLE instance ADD COLUMN machine INTEGER NOT NULL DEFAULT 0;
-- Superseded: machines have a real address on the lab network, so no port translation is needed.
ALTER TABLE instance DROP COLUMN guest_port;
ALTER TABLE instance DROP COLUMN lab_addr;
