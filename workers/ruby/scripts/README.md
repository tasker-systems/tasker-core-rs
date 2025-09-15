# Ruby Gem CI Scripts

This directory contains scripts to validate and test the GitHub Actions workflow locally before pushing to CI.

## Scripts

### `validate_workflow.sh`
**Purpose**: Validates the GitHub Actions workflow YAML file
**Speed**: Very fast (~2 seconds)
**What it checks**:
- YAML syntax validation
- Environment variables configuration
- Database configuration
- Required steps presence
- Matrix configuration
- Modern GitHub Actions usage

```bash
./scripts/validate_workflow.sh
```

### `ci_check.sh`
**Purpose**: Quick validation of CI requirements
**Speed**: Fast (~30 seconds)
**What it checks**:
- System dependencies (Rust, Ruby, PostgreSQL)
- Database connectivity
- Ruby dependencies
- Rust formatting and clippy
- Ruby extension compilation
- Test environment setup
- Gem building

```bash
./scripts/ci_check.sh
```

### `ci_dry_run.sh`
**Purpose**: Full simulation of the CI workflow
**Speed**: Slow (~5 minutes)
**What it does**:
- Everything from `ci_check.sh`
- Runs complete test suite
- Tests gem installation
- Provides detailed output

```bash
./scripts/ci_dry_run.sh
```

## Recommended Usage

1. **Before making workflow changes**:
   ```bash
   ./scripts/validate_workflow.sh
   ```

2. **Before pushing to CI**:
   ```bash
   ./scripts/ci_check.sh
   ```

3. **For complete validation**:
   ```bash
   ./scripts/ci_dry_run.sh
   ```

## Common Issues and Solutions

### PostgreSQL Connection Issues
If you get database connection errors:

```bash
# Create the user and database
createuser -s tasker
createdb -O tasker tasker_rust_test
psql -c "ALTER USER tasker WITH PASSWORD 'tasker';"
```

### Rust Formatting Issues
If Rust formatting fails:

```bash
cd ../..
cargo fmt
```

### Rust Clippy Issues
If clippy fails:

```bash
cd ../..
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### Ruby Dependencies Issues
If bundle install fails:

```bash
bundle install
```

## Environment Variables

The scripts use these environment variables (matching CI):

- `DATABASE_URL`: PostgreSQL connection string
- `TASKER_ENV`: Set to `test`
- `RAILS_ENV`: Set to `test`
- `APP_ENV`: Set to `test`
- `RACK_ENV`: Set to `test`

## Exit Codes

- `0`: Success
- `1`: Failure (check output for details)

All scripts use colored output for easy issue identification:
- ✅ Green: Success
- ❌ Red: Error
- ⚠️ Yellow: Warning