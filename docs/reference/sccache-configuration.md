# SCache Configuration Documentation

## Overview

This document records our sccache configuration for future reference. Sccache is currently **disabled** due to GitHub Actions cache service issues, but we plan to re-enable it once the service is stable.

## Current Status

🚫 **DISABLED** - Temporarily disabled due to GitHub Actions cache service issues:
```
sccache: error: Server startup failed: cache storage failed to read: Unexpected (permanent) at read => <h2>Our services aren't available right now</h2><p>We're working to restore all services as soon as possible. Please check back soon.</p>
```

## Planned Configuration

### Environment Variables (setup-env action)
```bash
RUSTC_WRAPPER=sccache
SCCACHE_GHA_ENABLED=true
SCCACHE_CACHE_SIZE=2G  # For Docker builds
```

### GitHub Actions Integration

#### Workflows Using sccache
1. **code-quality.yml** - Build caching for clippy and rustfmt
2. **test-unit.yml** - Build caching for unit tests 
3. **test-integration.yml** - Build caching for integration tests

#### Action Configuration
```yaml
- uses: mozilla-actions/sccache-action@v0.0.4
```

### Expected Benefits

- **50%+ faster builds** through compilation caching
- **Reduced CI costs** by avoiding redundant compilation
- **Better developer experience** with faster feedback loops

### Performance Targets

- **Build cache hit rate**: Target > 80%
- **Compilation time reduction**: 50%+ on cache hits
- **Total CI time**: Reduce by 10-20 minutes per run

## Local Development Setup

For local development when sccache is working:

```bash
# Install sccache
cargo binstall sccache -y

# Set environment variables
export RUSTC_WRAPPER=sccache
export SCCACHE_GHA_ENABLED=true

# Check stats
sccache --show-stats

# Clear cache if needed
sccache --zero-stats
```

## Re-enabling Steps

When GitHub Actions cache service is stable:

1. **Re-enable in workflows**:
   - Uncomment `mozilla-actions/sccache-action@v0.0.4` in workflows
   - Restore sccache environment variables in setup-env action

2. **Test with minimal workflow first**:
   - Start with code-quality.yml
   - Monitor for cache service issues
   - Gradually enable in other workflows

3. **Monitor performance**:
   - Track build times before/after
   - Monitor cache hit rates
   - Watch for any new cache service errors

## Configuration Locations

### Files containing sccache configuration:
- `.github/actions/setup-env/action.yml` - Environment variables
- `.github/workflows/code-quality.yml` - Action usage
- `.github/workflows/test-unit.yml` - Action usage  
- `.github/workflows/test-integration.yml` - Action usage
- `docs/sccache-configuration.md` - This documentation

### Docker Integration
For Docker builds, pass sccache variables as build args:
```yaml
build-args: |
  SCCACHE_GHA_ENABLED=true
  RUSTC_WRAPPER=sccache
  SCCACHE_CACHE_SIZE=2G
```

## Troubleshooting

### Common Issues
- **Cache service unavailable**: Wait for GitHub to restore service
- **Cache misses**: Check RUSTC_WRAPPER is set correctly
- **Permission errors**: Ensure sccache action has proper permissions

### Monitoring
- Check `sccache --show-stats` for cache effectiveness
- Monitor CI run times for performance improvements
- Watch GitHub status page for cache service updates

## References

- [sccache GitHub](https://github.com/mozilla/sccache)
- [mozilla-actions/sccache-action](https://github.com/mozilla-actions/sccache-action)
- [GitHub Actions Cache Service](https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows)