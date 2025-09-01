#!/bin/bash
# =============================================================================
# Tasker Core Development Database Runner
# =============================================================================
# Starts just the PostgreSQL database with PGMQ + UUID v7 extensions
# Perfect for local development when you only need the database running
# Uses the consolidated docker/ directory structure

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$DOCKER_DIR")"
COMPOSE_FILE="docker-compose.db.yml"

# Ensure we're in the docker directory for proper context
cd "$DOCKER_DIR"

echo "🐘 Tasker Core Development Database Manager"
echo "Docker directory: $DOCKER_DIR"
echo "Using compose file: $COMPOSE_FILE"
echo ""

case "${1:-start}" in
  start)
    echo "🚀 Starting PostgreSQL with PGMQ + UUID v7 extensions..."
    docker-compose -f "$COMPOSE_FILE" up -d postgres
    echo ""
    echo "⏳ Waiting for PostgreSQL to be ready..."
    
    # Wait for PostgreSQL to be healthy
    echo "Checking PostgreSQL health..."
    until docker-compose -f "$COMPOSE_FILE" exec postgres pg_isready -U tasker -d tasker_rust_test >/dev/null 2>&1; do
        echo "  ⏳ PostgreSQL starting up..."
        sleep 2
    done
    
    echo ""
    echo "✅ PostgreSQL ready!"
    echo ""
    echo "📋 Connection details:"
    echo "   DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
    echo "   Host: localhost"
    echo "   Port: 5432"
    echo "   Database: tasker_rust_test"
    echo "   Username: tasker"
    echo "   Password: tasker"
    echo ""
    echo "🔗 Extensions available:"
    echo "   • PGMQ (PostgreSQL Message Queue)"
    echo "   • UUID v7 (pg_uuidv7)"
    echo "   • Standard PostgreSQL extensions"
    echo ""
    echo "🧪 Test connection:"
    echo "   psql postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
    echo "   ./scripts/run-dev-db.sh psql"
    ;;

  stop)
    echo "🛑 Stopping PostgreSQL..."
    docker-compose -f "$COMPOSE_FILE" down
    echo "✅ PostgreSQL stopped"
    ;;

  restart)
    echo "🔄 Restarting PostgreSQL..."
    docker-compose -f "$COMPOSE_FILE" down
    sleep 2
    docker-compose -f "$COMPOSE_FILE" up -d postgres
    echo "⏳ Waiting for PostgreSQL to be ready..."
    until docker-compose -f "$COMPOSE_FILE" exec postgres pg_isready -U tasker -d tasker_rust_test >/dev/null 2>&1; do
        echo "  ⏳ PostgreSQL restarting..."
        sleep 2
    done
    echo "✅ PostgreSQL restarted and ready"
    ;;

  logs)
    echo "📋 PostgreSQL logs (Ctrl+C to exit):"
    docker-compose -f "$COMPOSE_FILE" logs -f postgres
    ;;

  psql)
    echo "🐘 Connecting to PostgreSQL shell..."
    echo "   Database: tasker_rust_test"
    echo "   User: tasker"
    echo ""
    docker-compose -f "$COMPOSE_FILE" exec postgres psql -U tasker -d tasker_rust_test
    ;;

  shell|bash)
    echo "🐧 Connecting to PostgreSQL container shell..."
    docker-compose -f "$COMPOSE_FILE" exec postgres bash
    ;;

  status)
    echo "📊 PostgreSQL container status:"
    docker-compose -f "$COMPOSE_FILE" ps postgres
    echo ""
    echo "🔍 Health check:"
    if docker-compose -f "$COMPOSE_FILE" exec postgres pg_isready -U tasker -d tasker_rust_test >/dev/null 2>&1; then
        echo "   ✅ PostgreSQL is healthy and accepting connections"
    else
        echo "   ❌ PostgreSQL is not ready or not running"
    fi
    
    echo ""
    echo "🔌 Port status:"
    if nc -z localhost 5432 2>/dev/null; then
        echo "   ✅ Port 5432 is open and listening"
    else
        echo "   ❌ Port 5432 is not accessible"
    fi
    ;;

  test)
    echo "🧪 Testing PostgreSQL and extensions..."
    
    # Test basic connection
    echo "1. Testing basic connection..."
    if docker-compose -f "$COMPOSE_FILE" exec postgres pg_isready -U tasker -d tasker_rust_test >/dev/null 2>&1; then
        echo "   ✅ Basic connection works"
    else
        echo "   ❌ Basic connection failed"
        exit 1
    fi
    
    # Test PGMQ extension
    echo "2. Testing PGMQ extension..."
    if docker-compose -f "$COMPOSE_FILE" exec postgres psql -U tasker -d tasker_rust_test -c "SELECT pgmq.create('test_queue');" >/dev/null 2>&1; then
        echo "   ✅ PGMQ extension works"
        docker-compose -f "$COMPOSE_FILE" exec postgres psql -U tasker -d tasker_rust_test -c "SELECT pgmq.drop_queue('test_queue');" >/dev/null 2>&1
    else
        echo "   ❌ PGMQ extension failed"
        exit 1
    fi
    
    # Test UUID v7 extension
    echo "3. Testing UUID v7 extension..."
    if docker-compose -f "$COMPOSE_FILE" exec postgres psql -U tasker -d tasker_rust_test -c "SELECT uuid_generate_v7();" >/dev/null 2>&1; then
        echo "   ✅ UUID v7 extension works"
    else
        echo "   ❌ UUID v7 extension failed"
        exit 1
    fi
    
    echo ""
    echo "✅ All tests passed! Database is ready for Tasker development."
    ;;

  reset)
    echo "⚠️  Resetting PostgreSQL data (this will delete all data)..."
    echo "This will:"
    echo "  • Stop the PostgreSQL container"
    echo "  • Delete all database volumes and data"
    echo "  • Restart with fresh database"
    echo ""
    read -p "Are you sure you want to reset all data? [y/N] " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "🗑️  Removing PostgreSQL containers and volumes..."
        docker-compose -f "$COMPOSE_FILE" down -v
        echo "🚀 Starting fresh PostgreSQL instance..."
        docker-compose -f "$COMPOSE_FILE" up -d postgres
        echo "⏳ Waiting for PostgreSQL to initialize..."
        until docker-compose -f "$COMPOSE_FILE" exec postgres pg_isready -U tasker -d tasker_rust_test >/dev/null 2>&1; do
            echo "  ⏳ PostgreSQL initializing..."
            sleep 3
        done
        echo "✅ PostgreSQL reset complete with fresh data"
    else
        echo "❌ Reset cancelled"
    fi
    ;;

  build)
    echo "🔨 Building custom PostgreSQL image with extensions..."
    docker-compose -f "$COMPOSE_FILE" build postgres
    echo "✅ PostgreSQL image built successfully"
    echo ""
    echo "🔍 Image details:"
    docker images | grep postgres | head -3
    ;;

  *)
    echo "Usage: $0 {start|stop|restart|logs|psql|shell|status|test|reset|build}"
    echo ""
    echo "Commands:"
    echo "  start    - Start PostgreSQL service (default)"
    echo "  stop     - Stop PostgreSQL service"
    echo "  restart  - Restart PostgreSQL service"
    echo "  logs     - Show PostgreSQL logs"
    echo "  psql     - Connect to PostgreSQL shell"
    echo "  shell    - Connect to container bash shell"
    echo "  status   - Show container and connection status"
    echo "  test     - Test database and extensions"
    echo "  reset    - Reset PostgreSQL data (destructive!)"
    echo "  build    - Build custom PostgreSQL image"
    echo ""
    echo "Examples:"
    echo "  $0 start           # Start database"
    echo "  $0 test            # Verify database and extensions work"
    echo "  $0 psql            # Connect to database shell"
    echo "  $0 logs            # Watch database logs"
    echo ""
    echo "🔗 Connection string:"
    echo "  DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
    exit 1
    ;;
esac