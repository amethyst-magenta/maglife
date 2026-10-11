ALTER TABLE template_blocks ADD COLUMN ends_next_day BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE template_blocks DROP CONSTRAINT valid_template_time;
ALTER TABLE template_blocks ADD CONSTRAINT valid_template_time CHECK (
    (NOT ends_next_day AND ends_at > starts_at)
    OR (ends_next_day AND ends_at = TIME '00:00')
);

ALTER TABLE fact_entries ALTER COLUMN actual_start DROP NOT NULL;
ALTER TABLE fact_entries DROP CONSTRAINT valid_fact_state;
ALTER TABLE fact_entries ADD CONSTRAINT valid_fact_state CHECK (
    (status = 'running' AND actual_start IS NOT NULL AND actual_end IS NULL)
    OR (status = 'completed' AND actual_start IS NOT NULL AND actual_end IS NOT NULL)
    OR (status = 'skipped' AND plan_block_id IS NOT NULL AND actual_start IS NULL AND actual_end IS NULL)
);
ALTER TABLE fact_entries ADD CONSTRAINT fact_day_fk FOREIGN KEY (day) REFERENCES day_plans(day);
ALTER TABLE plan_blocks ADD CONSTRAINT plan_block_day_unique UNIQUE (id, day);
ALTER TABLE fact_entries ADD CONSTRAINT fact_plan_day_fk FOREIGN KEY (plan_block_id, day) REFERENCES plan_blocks(id, day);

CREATE FUNCTION guard_approved_plan() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    parent_state TEXT;
BEGIN
    -- Lock the parent so approval and draft edits cannot race.
    SELECT state INTO parent_state FROM day_plans WHERE day = COALESCE(NEW.day, OLD.day) FOR UPDATE;
    IF parent_state = 'approved' THEN
        RAISE EXCEPTION 'Approved plans cannot be edited' USING ERRCODE = '23514', CONSTRAINT = 'immutable_plan';
    END IF;
    IF TG_OP = 'UPDATE' AND NEW.day <> OLD.day THEN
        RAISE EXCEPTION 'Plan blocks cannot change day' USING ERRCODE = '23514';
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER protect_plan_blocks BEFORE INSERT OR UPDATE OR DELETE ON plan_blocks
    FOR EACH ROW EXECUTE FUNCTION guard_approved_plan();

CREATE FUNCTION guard_plan_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.state = 'approved' AND (TG_OP = 'DELETE' OR NEW.state <> 'approved') THEN
        RAISE EXCEPTION 'Approved plans cannot be reopened' USING ERRCODE = '23514', CONSTRAINT = 'immutable_plan';
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER protect_day_plan BEFORE UPDATE OR DELETE ON day_plans
    FOR EACH ROW EXECUTE FUNCTION guard_plan_state();

CREATE UNIQUE INDEX one_fact_per_plan_block ON fact_entries (plan_block_id)
    WHERE plan_block_id IS NOT NULL;
