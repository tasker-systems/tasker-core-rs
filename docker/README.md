# Tasker System Docker Infrastructure

Comprehensive Docker infrastructure for the Tasker Core system with production-ready deployment, automated migrations, and scalable worker architecture.

## 🏗️ Current Architecture

### Directory Structure

```
docker/
├── build/                              # Service-specific Dockerfiles
│   ├── orchestration.test.Dockerfile   # Test build: orchestration + migrations
│   ├── orchestration.prod.Dockerfile   # Production build: orchestration + migrations
│   ├── rust-worker.test.Dockerfile     # Test build: worker (no migrations)
│   └── rust-worker.prod.Dockerfile     # Production build: worker (no migrations)
├── db/
│   └── Dockerfile                      # PostgreSQL with PGMQ + UUID v7 extensions
├── scripts/                            # Service entrypoints and utilities
│   ├── orchestration-entrypoint.sh     # Orchestration startup + migrations
│   ├── worker-entrypoint.sh             # Worker startup (migration-free)
│   └── migrate.sh                      # Database migration utility
├── docker-compose.test.yml             # Test environment
├── docker-compose.prod.yml             # Production environment
├── .env.prod.template                  # Production environment template
└── README.md                           # This documentation
```

### Service Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           TASKER SYSTEM                                │
├─────────────────────────────────────────────────────────────────────────┤
│ PostgreSQL Database                                                     │
│ ├── PGMQ Extension (message queues)                                    │
│ ├── UUID v7 Extension (time-ordered UUIDs)                            │
│ └── Tasker Schema (managed by orchestration)                          │
├─────────────────────────────────────────────────────────────────────────┤
│ Orchestration Service                                                   │
│ ├── Database Migration Owner (SQLx CLI + scripts)                     │
│ ├── Task Orchestration & Coordination                                 │
│ ├── Web API (health checks, task management)                          │
│ └── Event System (PGMQ notifications)                                 │
├─────────────────────────────────────────────────────────────────────────┤
│ Worker Service(s) - Horizontally Scalable                             │
│ ├── Task Execution Engine                                             │
│ ├── Task Template Management                                           │
│ ├── Event-Driven Processing                                           │
│ ├── Web API (health checks, metrics)                                  │
│ └── No Migration Logic (faster startup)                               │
└─────────────────────────────────────────────────────────────────────────┘
```

## 🔧 Service Responsibilities

### Orchestration Service
**Purpose**: Database schema owner and task orchestration

**Responsibilities:**
- ✅ Database migrations (CREATE/ALTER schema)
- ✅ PGMQ extension dependency verification
- ✅ Production deployment mode handling
- ✅ Migration retry logic and timeouts
- ✅ Environment validation (DATABASE_URL, etc.)
- ✅ Task orchestration and coordination
- ✅ Web API (health checks, task management)

**Entrypoint**: `orchestration-entrypoint.sh`
**Docker Images**: Includes SQLx CLI, PostgreSQL client, migration scripts

### Worker Service
**Purpose**: Task execution engine

**Responsibilities:**
- ✅ Task execution and processing
- ✅ Task template management
- ✅ Event-driven processing (PGMQ)
- ✅ Worker-specific health checks
- ✅ Configuration validation
- ❌ **No database migration logic** (faster startup, scalable)

**Entrypoint**: `worker-entrypoint.sh`
**Docker Images**: Minimal runtime, no migration tools

## 🚀 Key Features

### Database Migration Strategy
- **Single Source of Truth**: Only orchestration service manages database schema
- **Production Safety**: Configurable timeouts, retry logic, deployment modes
- **Zero-Conflict Scaling**: Workers can start simultaneously without migration conflicts
- **Deployment Modes**: `standard`, `migrate-only`, `no-migrate` for different scenarios

### Performance Optimizations
- **cargo-chef integration**: Docker layer caching for faster Rust builds
- **Multi-stage builds**: Minimal runtime images with optimized binaries
- **Service separation**: Workers start faster without migration overhead
- **Horizontal scaling**: Scale workers independently

### Production Ready
- **Non-root execution**: All containers run as unprivileged `tasker` user
- **Health checks**: Comprehensive monitoring for all services
- **Resource limits**: Configurable CPU and memory constraints
- **Environment validation**: Required variables checked at startup

## 🚀 Quick Start

### Test Environment
```bash
cd docker
docker compose -f docker-compose.test.yml up --build
```
**Features:** Debug builds, orchestration handles migrations, worker scaling ready

### Production Deployment
```bash
cd docker

# 1. Configure environment
cp .env.prod.template .env.prod
vim .env.prod  # Set POSTGRES_PASSWORD and other values

# 2. Deploy
docker compose -f docker-compose.prod.yml --env-file .env.prod up -d

# 3. Scale workers as needed
docker compose -f docker-compose.prod.yml --env-file .env.prod up -d --scale worker=3
```

### Key Environment Variables

**Required:**
```bash
# Database Configuration
POSTGRES_PASSWORD=your_secure_password
DATABASE_URL=postgresql://tasker:password@postgres:5432/tasker_production

# Application
TASKER_ENV=production  # or test
WORKER_ID=prod-worker-001
```

**Migration Control:**
```bash
# Orchestration service (default: migrations enabled)
RUN_MIGRATIONS=true
DEPLOYMENT_MODE=standard  # standard|migrate-only|no-migrate
DB_MIGRATION_TIMEOUT=300
DB_MIGRATION_RETRIES=3

# Workers (no migration variables needed)
TASKER_TEMPLATE_PATH=/app/task-templates
```

## 📊 Deployment Patterns

### Single-Instance Deployment (Recommended)
```yaml
services:
  orchestration:
    environment:
      RUN_MIGRATIONS: "true"
      DEPLOYMENT_MODE: standard
    depends_on:
      postgres:
        condition: service_healthy

  worker:
    depends_on:
      orchestration:
        condition: service_healthy
    deploy:
      replicas: 2  # Scale as needed
```

### Advanced: Migration-Only Container
```yaml
services:
  migration:
    image: orchestration:latest
    environment:
      DEPLOYMENT_MODE: migrate-only
    restart: "no"

  orchestration:
    environment:
      DEPLOYMENT_MODE: no-migrate
    depends_on:
      migration:
        condition: service_completed_successfully
```

### Health Checks & Monitoring
```bash
# Service health
curl http://localhost:8080/health  # Orchestration
curl http://localhost:8081/health  # Worker

# Migration status
docker compose exec orchestration sqlx migrate info

# Service logs
docker compose logs orchestration | grep "\[ORCHESTRATION\]"
docker compose logs worker | grep "\[WORKER\]"
```

## 💡 Database Migration Strategy

### Migration Modes

1. **Standard Mode (Default)**
   - Orchestration runs migrations before starting
   - Recommended for most deployments
   ```yaml
   environment:
     DEPLOYMENT_MODE: standard
     RUN_MIGRATIONS: "true"
   ```

2. **Migrate-Only Mode**
   - Run migrations and exit (init containers)
   - Perfect for blue-green deployments
   ```yaml
   environment:
     DEPLOYMENT_MODE: migrate-only
   ```

3. **No-Migrate Mode**
   - Skip migrations entirely
   - Workers always use this mode
   ```yaml
   environment:
     DEPLOYMENT_MODE: no-migrate
   ```

### Migration Safety Features
- **Timeout Protection**: Configurable migration timeout (default: 5 minutes)
- **Retry Logic**: Automatic retry for transient failures
- **PGMQ Prerequisites**: Validates PGMQ extension before migration
- **Status Reporting**: Comprehensive migration logging with `[INFO]`, `[SUCCESS]`, `[ERROR]` tags

### Production Migration Best Practices

1. **Always test migrations on staging first**
2. **Backup database before production migrations**
3. **Monitor migration logs during deployment**
4. **Set appropriate timeouts for large migrations**
5. **Use `DEPLOYMENT_MODE=migrate-only` for complex deployments**

## 🛡️ Production Scaling

### Resource Limits

Production deployments include configured resource limits:

```yaml
# Orchestration Service
deploy:
  resources:
    limits:
      cpus: "4"
      memory: 2G
    reservations:
      cpus: "1"
      memory: 512M

# Worker Service
deploy:
  resources:
    limits:
      cpus: "8"
      memory: 4G
    reservations:
      cpus: "2"
      memory: 1G
  replicas: 2  # Default worker count
```

### Scaling Commands

```bash
# Scale workers up
docker compose -f docker-compose.prod.yml up -d --scale worker=5

# Scale workers down
docker compose -f docker-compose.prod.yml up -d --scale worker=1

# Check current scaling
docker compose -f docker-compose.prod.yml ps
```

## 🔧 Troubleshooting

### Migration Issues

**Migration timeout:**
```bash
# Increase timeout for large migrations
DB_MIGRATION_TIMEOUT=600
DB_MIGRATION_RETRIES=5
```

**PGMQ extension not found:**
```bash
# Verify PGMQ extension is installed
docker compose exec postgres psql -U tasker -d tasker_production -c "CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;"
```

**Migration conflicts:**
```bash
# Clean database restart
docker compose down -v
docker compose up
```

### Service Issues

**Health check failures:**
```bash
# Check service logs with structured output
docker compose logs orchestration | grep "\[ORCHESTRATION\]"
docker compose logs worker | grep "\[WORKER\]"

# Check specific health endpoints
curl -f http://localhost:8080/health  # Should return 200
curl -f http://localhost:8081/health  # Should return 200
```

**Database connectivity:**
```bash
# Verify PostgreSQL health
docker compose exec postgres pg_isready -U tasker

# Test database from orchestration container
docker compose exec orchestration psql $DATABASE_URL -c "SELECT 1;"
```

**Build issues:**
```bash
# Clear Docker build cache
docker builder prune

# Rebuild from scratch
docker compose build --no-cache
```

### Debug Commands

```bash
# Check migration status
docker compose exec orchestration sqlx migrate info --database-url $DATABASE_URL

# Manual migration run
docker compose exec orchestration sqlx migrate run --database-url $DATABASE_URL

# View container resource usage
docker stats

# Inspect container configuration
docker compose config
```

## 📝 Production Checklist

Before deploying to production:

- [ ] 📝 **Environment file configured** (`.env.prod` with secure passwords)
- [ ] 🛡️ **Database backups configured**
- [ ] 🔍 **PGMQ extension available** in database
- [ ] 📁 **Config files mounted correctly** (`/app/config/tasker`)
- [ ] ❤️ **Health checks responding** (orchestration + workers)
- [ ] 📊 **Log aggregation configured**
- [ ] 📊 **Resource limits appropriate** for workload
- [ ] 🔄 **Worker scaling tested** (`--scale worker=N`)

## 🚀 Summary

**Current State**: Production-ready Docker infrastructure with:

✅ **Clean Service Separation**: Orchestration owns migrations, workers focus on execution
✅ **Zero-Conflict Scaling**: Multiple workers start without migration conflicts
✅ **Production Safety**: Timeout protection, retry logic, deployment modes
✅ **Health Monitoring**: Comprehensive health checks and structured logging
✅ **Resource Management**: Configurable limits and horizontal scaling

This infrastructure provides a **robust, scalable foundation** for the Tasker Core ecosystem with clear service boundaries and production-grade deployment capabilities. 🎆
