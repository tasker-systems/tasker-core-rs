# frozen_string_literal: true

module TaskerCore
  module Utils
    # Centralized utility for consistent path resolution across TaskerCore
    #
    # This class provides a single source of truth for:
    # - Finding the project root directory
    # - Resolving relative paths consistently
    # - Validating search paths
    # - Providing helpful error messages
    #
    # @example Basic usage
    #   PathResolver.project_root
    #   #=> "/Users/user/projects/tasker-core-rs"
    #
    # @example Resolving config paths
    #   PathResolver.resolve_config_path("config/tasks/*.yaml")
    #   #=> "/Users/user/projects/tasker-core-rs/config/tasks/*.yaml"
    #
    # @example Validating search paths
    #   PathResolver.validate_search_paths(["config/*.yaml", "invalid/path/*.yaml"])
    #   # Logs warnings for paths that yield no matches
    #
    class PathResolver
      class << self
        # Get the project root directory
        # @return [String] Absolute path to project root
        def project_root
          @project_root ||= find_project_root
        end

        # Get the gem root directory (same as project root for Gemfile-based apps)
        # @return [String] Absolute path to gem root
        def gem_root
          @gem_root ||= project_root
        end

        # Get the gem library root directory
        # @return [String] Absolute path to gem library root
        def gem_lib_root
          # In a Gemfile-based app, the gem is installed via bundler
          # So we need to find where this gem is actually installed
          @gem_lib_root ||= File.expand_path(File.dirname(__dir__))
        end

        # Reset cached project root (useful for testing)
        def reset!
          @project_root = nil
        end

        # Resolve a relative path from the project root
        # @param relative_path [String] Path relative to project root
        # @return [String] Absolute path
        def resolve_config_path(relative_path)
          return relative_path if absolute_path?(relative_path)

          File.expand_path(relative_path, project_root)
        end

        # Validate search paths and warn about paths that yield no matches
        # @param paths [Array<String>] Array of glob patterns to validate
        # @return [Hash] Summary of validation results
        def validate_search_paths(paths)
          results = {
            total_paths: paths.length,
            valid_paths: 0,
            total_files: 0,
            warnings: []
          }

          paths.each do |path|
            expanded_path = resolve_config_path(path)
            files = Dir.glob(expanded_path)

            if files.any?
              results[:valid_paths] += 1
              results[:total_files] += files.length
              logger&.debug "✅ Search path valid: #{expanded_path} (#{files.length} files)"
            else
              warning = "Search path yields no matches: #{expanded_path}"
              results[:warnings] << warning
              logger&.warn "⚠️  #{warning}"
            end
          end

          results
        end

        # Check if we're running in a development context
        # @return [Boolean] True if in development mode
        def development_mode?
          env = ENV['TASKER_ENV'] || ENV['RAILS_ENV'] || 'development'
          env == 'development'
        end

        # Get relative path from project root
        # @param absolute_path [String] Absolute path
        # @return [String] Path relative to project root
        def relative_path_from_root(absolute_path)
          Pathname.new(absolute_path).relative_path_from(Pathname.new(project_root)).to_s
        rescue ArgumentError
          absolute_path # Return original if can't make relative
        end

        # Check if a path exists relative to project root
        # @param relative_path [String] Path relative to project root
        # @return [Boolean] True if path exists
        def path_exists?(relative_path)
          File.exist?(resolve_config_path(relative_path))
        end

        # Find all files matching a pattern relative to project root
        # @param pattern [String] Glob pattern relative to project root
        # @return [Array<String>] Array of absolute paths
        def find_files(pattern)
          Dir.glob(resolve_config_path(pattern))
        end

        # Get project structure summary for diagnostics
        # @return [Hash] Summary of project structure
        def project_structure_summary
          root = project_root
          {
            project_root: root,
            exists: File.directory?(root),
            gemfile: File.exist?(File.join(root, 'Gemfile')),
            gemfile_lock: File.exist?(File.join(root, 'Gemfile.lock')),
            config_dir: File.directory?(File.join(root, 'config')),
            tasker_config_dir: File.directory?(File.join(root, 'config', 'tasker')),
            component_config: File.directory?(File.join(root, 'config', 'tasker', 'environments')),
            working_directory: Dir.pwd,
            relative_working_dir: relative_path_from_root(Dir.pwd)
          }
        end

        private

        # Check if a path is absolute (cross-platform)
        # @param path [String] Path to check
        # @return [Boolean] True if path is absolute
        def absolute_path?(path)
          return false if path.nil? || path.empty?

          # Unix/Linux/macOS: starts with /
          return true if path.start_with?('/')

          # Windows: starts with drive letter (C:) or UNC path (\\)
          return true if path.match?(/\A[a-zA-Z]:/) || path.start_with?('\\\\')

          false
        end

        # Find the project root by walking up the directory tree
        # For Ruby bindings, we ONLY look for Gemfile (application root)
        # @return [String] Absolute path to project root (Gemfile-root only)
        def find_project_root
          # Start from current file location
          current_dir = __dir__

          logger&.debug "🔍 PROJECT_ROOT: Starting search from: #{current_dir}"

          # Walk up directory tree looking for Gemfile (Ruby application root)
          max_levels = 10 # Prevent infinite loops
          max_levels.times do |level|
            logger&.debug "🔍 PROJECT_ROOT: Checking level #{level}: #{current_dir}"

            # Check for Gemfile (Ruby application root)
            gemfile_path = File.join(current_dir, 'Gemfile')
            if File.exist?(gemfile_path)
              logger&.debug "✅ PROJECT_ROOT: Found Gemfile at: #{current_dir}"
              return current_dir
            end

            # Check if we've reached the filesystem root
            parent = File.dirname(current_dir)
            if parent == current_dir
              logger&.debug '🚫 PROJECT_ROOT: Reached filesystem root'
              break
            end

            current_dir = parent
          end

          # Final fallback: Use working directory (but warn strongly)
          fallback = Dir.pwd
          logger&.error "❌ PROJECT_ROOT: No Gemfile found! Using working directory as last resort: #{fallback}"
          logger&.error '❌ PROJECT_ROOT: Configuration may not load correctly without a Gemfile'
          logger&.error '❌ PROJECT_ROOT: Please ensure you are running from a Ruby application directory'
          fallback
        end

        # Get logger instance if available
        # @return [Logger, nil] Logger instance or nil
        def logger
          @logger ||= TaskerCore::Logging::Logger.instance
        end
      end
    end
  end
end
