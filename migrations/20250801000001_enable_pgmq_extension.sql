-- Enable pgmq extension for PostgreSQL message queue functionality
--
-- pgmq provides SQS-like message queue operations directly in PostgreSQL:
-- - pgmq_send() - Send messages to queues
-- - pgmq_read() - Read messages with visibility timeout
-- - pgmq_delete() - Delete processed messages
-- - pgmq_archive() - Archive messages for retention
--
-- Installation requirements:
-- 1. pgmq must be installed via pgxn: `pgxn install pgmq`
-- 2. Extension must be enabled in database: `CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;`
--
-- This migration enables pgmq for queue-based workflow orchestration,
-- replacing the TCP command architecture with PostgreSQL-backed message queues.

-- Enable pgmq extension with CASCADE to install dependencies
CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;