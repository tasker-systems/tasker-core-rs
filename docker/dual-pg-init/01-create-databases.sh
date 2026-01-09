#!/bin/bash
# TAS-78: Initialize dual database setup for split-database testing
#
# Creates:
# - tasker_split_test: Tasker tables (NO pgmq extension - proves isolation)
# - pgmq_split_test: PGMQ message queues only
#
# This setup validates that:
# 1. Tasker and PGMQ can operate on separate databases
# 2. Code doesn't accidentally depend on pgmq.metrics() in Tasker DB
# 3. Views/functions gracefully handle missing PGMQ schema

set -e

echo "============================================="
echo "TAS-78: Setting up dual database environment"
echo "============================================="

# Create the Tasker database (WITHOUT pgmq extension)
echo "Creating tasker_split_test database..."
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" <<-EOSQL
    CREATE DATABASE tasker_split_test;
EOSQL

# Create the PGMQ database (WITH pgmq extension)
echo "Creating pgmq_split_test database..."
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" <<-EOSQL
    CREATE DATABASE pgmq_split_test;
EOSQL

# Setup Tasker database - pg_uuidv7 only, NO PGMQ
echo "Configuring tasker_split_test (pg_uuidv7 only)..."
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname=tasker_split_test <<-EOSQL
    -- Only pg_uuidv7 for UUID support
    CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;

    -- Verify extension
    SELECT uuid_generate_v7() as test_uuid;

    -- Explicitly verify PGMQ is NOT available
    DO \$\$
    BEGIN
        IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pgmq') THEN
            RAISE EXCEPTION 'PGMQ should NOT be installed in tasker_split_test!';
        END IF;
        RAISE NOTICE 'Verified: PGMQ not installed in tasker_split_test (expected)';
    END
    \$\$;
EOSQL

# Setup PGMQ database - both extensions
echo "Configuring pgmq_split_test (pgmq + pg_uuidv7)..."
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname=pgmq_split_test <<-EOSQL
    -- PGMQ extension (pulls in dependencies)
    CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;

    -- pg_uuidv7 for UUID support in messages
    CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;

    -- Verify extensions
    SELECT
        (SELECT count(*) FROM pg_extension WHERE extname = 'pgmq') as pgmq_installed,
        (SELECT count(*) FROM pg_extension WHERE extname = 'pg_uuidv7') as uuidv7_installed;

    -- Create a test queue to verify PGMQ works
    SELECT pgmq.create('test_queue');
    SELECT pgmq.send('test_queue', '{"test": "message"}'::jsonb);
    SELECT pgmq.purge_queue('test_queue');

    RAISE NOTICE 'PGMQ verified working in pgmq_split_test';
EOSQL

echo ""
echo "============================================="
echo "Dual database setup complete!"
echo "============================================="
echo ""
echo "Tasker DB: postgresql://tasker:tasker@localhost:5433/tasker_split_test"
echo "  - pg_uuidv7: YES"
echo "  - pgmq: NO (intentionally excluded)"
echo ""
echo "PGMQ DB:   postgresql://tasker:tasker@localhost:5433/pgmq_split_test"
echo "  - pg_uuidv7: YES"
echo "  - pgmq: YES"
echo ""
