CREATE VIEW session_summary AS
SELECT status, COUNT(*) AS total, COALESCE(SUM(EXTRACT(EPOCH FROM (COALESCE(actual_end, NOW()) - actual_start)) / 60) FILTER (WHERE actual_start IS NOT NULL), 0) AS minutes
FROM sessions GROUP BY status;

CREATE VIEW event_summary AS
SELECT kind, COUNT(*) AS total FROM events GROUP BY kind ORDER BY total DESC;
