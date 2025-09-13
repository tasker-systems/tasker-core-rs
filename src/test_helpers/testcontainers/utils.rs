//! # Build Mode Configuration for testcontainers
//!
//! Provides flexible build mode configuration for different testing environments:
//! - Fast: Local development with debug builds
//! - CI: CI pipeline with sccache and GitHub cache
//! - Production: Full optimization with cargo-chef

use std::env;

/// Build mode configuration for Docker containers in tests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    /// Fast local development: debug builds, no optimization
    /// - Uses Dockerfile.test
    /// - Debug builds for speed
    /// - No external dependencies
    Fast,
    
    /// CI pipeline: sccache + GitHub cache
    /// - Uses Dockerfile.ci
    /// - sccache for distributed caching
    /// - GitHub Actions integration
    CI,
    
    /// Production: cargo-chef + full optimization
    /// - Uses Dockerfile.prod
    /// - Full release optimization
    /// - Layer caching with chef
    Production,
}

impl BuildMode {
    /// Detect build mode from environment variables
    /// 
    /// Uses TEST_BUILD_MODE environment variable:
    /// - "ci" -> BuildMode::CI
    /// - "prod" or "production" -> BuildMode::Production
    /// - Default -> BuildMode::Fast (for local development)
    pub fn from_env() -> Self {
        match env::var("TEST_BUILD_MODE").as_deref() {
            Ok("ci") => BuildMode::CI,
            Ok("prod") | Ok("production") => BuildMode::Production,
            _ => BuildMode::Fast, // Default for local development
        }
    }
    
    /// Get the corresponding Dockerfile path for this build mode
    pub fn dockerfile(&self) -> &'static str {
        match self {
            BuildMode::Fast => "docker/build/Dockerfile.test",
            BuildMode::CI => "docker/build/Dockerfile.ci", 
            BuildMode::Production => "docker/build/Dockerfile.prod",
        }
    }
    
    /// Get a human-readable description of this build mode
    pub fn description(&self) -> &'static str {
        match self {
            BuildMode::Fast => "Fast local development builds",
            BuildMode::CI => "CI pipeline with caching",
            BuildMode::Production => "Production optimized builds",
        }
    }
    
    /// Whether this build mode includes optimization
    pub fn is_optimized(&self) -> bool {
        matches!(self, BuildMode::CI | BuildMode::Production)
    }
    
    /// Whether this build mode uses external caching
    pub fn uses_caching(&self) -> bool {
        !matches!(self, BuildMode::Fast)
    }
}

impl Default for BuildMode {
    fn default() -> Self {
        Self::from_env()
    }
}

impl std::fmt::Display for BuildMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildMode::Fast => write!(f, "fast"),
            BuildMode::CI => write!(f, "ci"),
            BuildMode::Production => write!(f, "production"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_mode_from_env() {
        // Test default (Fast)
        env::remove_var("TEST_BUILD_MODE");
        assert_eq!(BuildMode::from_env(), BuildMode::Fast);
        
        // Test CI mode
        env::set_var("TEST_BUILD_MODE", "ci");
        assert_eq!(BuildMode::from_env(), BuildMode::CI);
        
        // Test Production modes
        env::set_var("TEST_BUILD_MODE", "prod");
        assert_eq!(BuildMode::from_env(), BuildMode::Production);
        
        env::set_var("TEST_BUILD_MODE", "production");
        assert_eq!(BuildMode::from_env(), BuildMode::Production);
        
        // Test invalid value defaults to Fast
        env::set_var("TEST_BUILD_MODE", "invalid");
        assert_eq!(BuildMode::from_env(), BuildMode::Fast);
        
        // Cleanup
        env::remove_var("TEST_BUILD_MODE");
    }
    
    #[test]
    fn test_dockerfile_paths() {
        assert_eq!(BuildMode::Fast.dockerfile(), "docker/build/Dockerfile.test");
        assert_eq!(BuildMode::CI.dockerfile(), "docker/build/Dockerfile.ci");
        assert_eq!(BuildMode::Production.dockerfile(), "docker/build/Dockerfile.prod");
    }
    
    #[test]
    fn test_build_mode_properties() {
        assert!(!BuildMode::Fast.is_optimized());
        assert!(BuildMode::CI.is_optimized());
        assert!(BuildMode::Production.is_optimized());
        
        assert!(!BuildMode::Fast.uses_caching());
        assert!(BuildMode::CI.uses_caching());
        assert!(BuildMode::Production.uses_caching());
    }
    
    #[test]
    fn test_display() {
        assert_eq!(BuildMode::Fast.to_string(), "fast");
        assert_eq!(BuildMode::CI.to_string(), "ci");
        assert_eq!(BuildMode::Production.to_string(), "production");
    }
}