-- A virtual machine offers several services (a web server and SSH), and takes longer to boot than a
-- container. The extra services are kept as JSON; ready_secs is how long this run may take to
-- answer before it counts as failed (empty means the usual minute).
ALTER TABLE instance ADD COLUMN extra_ports TEXT;
ALTER TABLE instance ADD COLUMN ready_secs INTEGER;
