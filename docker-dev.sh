#!/bin/bash
# Development helper script for Docker Compose PostgreSQL

set -e

COMPOSE_FILE="docker-compose.yml"

case "${1:-start}" in
  start)
    echo "🚀 Starting PostgreSQL with PGMQ + UUID v7 extensions..."
    docker-compose up -d postgres
    echo "⏳ Waiting for PostgreSQL to be ready..."
    docker-compose exec postgres pg_isready -U tasker -d tasker_rust_test
    echo "✅ PostgreSQL ready at localhost:5432"
    echo "📋 Connection details:"
    echo "   DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
    ;;
  
  stop)
    echo "🛑 Stopping PostgreSQL..."
    docker-compose down
    ;;
  
  restart)
    echo "🔄 Restarting PostgreSQL..."
    docker-compose down
    docker-compose up -d postgres
    ;;
  
  logs)
    echo "📋 PostgreSQL logs:"
    docker-compose logs -f postgres
    ;;
  
  psql)
    echo "🐘 Connecting to PostgreSQL..."
    docker-compose exec postgres psql -U tasker -d tasker_rust_test
    ;;
  
  reset)
    echo "⚠️  Resetting PostgreSQL data (this will delete all data)..."
    read -p "Are you sure? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
      docker-compose down -v
      docker-compose up -d postgres
      echo "✅ PostgreSQL reset complete"
    else
      echo "❌ Reset cancelled"
    fi
    ;;
  
  build)
    echo "🔨 Building custom PostgreSQL image..."
    docker-compose build postgres
    ;;
  
  *)
    echo "Usage: $0 {start|stop|restart|logs|psql|reset|build}"
    echo ""
    echo "Commands:"
    echo "  start    - Start PostgreSQL service (default)"
    echo "  stop     - Stop PostgreSQL service" 
    echo "  restart  - Restart PostgreSQL service"
    echo "  logs     - Show PostgreSQL logs"
    echo "  psql     - Connect to PostgreSQL shell"
    echo "  reset    - Reset PostgreSQL data (destructive!)"
    echo "  build    - Build custom PostgreSQL image"
    exit 1
    ;;
esac