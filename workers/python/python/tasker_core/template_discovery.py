"""Template discovery for YAML-based handler registration.

This module provides classes for discovering step handlers from YAML task
template files. It mirrors the Ruby template discovery pattern for consistency
across language implementations.

The discovery process follows this priority order:
1. TASKER_TEMPLATE_PATH environment variable (explicit override)
2. Test environment: tests/fixtures/task_templates/python/
3. Workspace detection + config/tasks
4. config/tasks fallback

Example:
    >>> from tasker_core.template_discovery import HandlerDiscovery, TemplatePath
    >>>
    >>> # Find template directory
    >>> template_dir = TemplatePath.find_template_config_directory()
    >>> if template_dir:
    ...     handlers = HandlerDiscovery.discover_all_handlers(template_dir)
    ...     print(f"Discovered {len(handlers)} handlers")
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

import yaml

from .logging import log_debug, log_info, log_warn

# Workspace marker files for detecting project root
WORKSPACE_MARKERS = (
    "Cargo.toml",
    "pyproject.toml",
    ".git",
    "Gemfile",
    "package.json",
    "tasker-core.code-workspace",
)


class TemplatePath:
    """Discovers template configuration directory.

    Provides methods to find the directory containing YAML task template
    files based on environment variables and workspace detection.

    The search follows this priority:
    1. TASKER_TEMPLATE_PATH env var (explicit override)
    2. Test environment defaults
    3. WORKSPACE_PATH/config/tasks
    4. Workspace auto-detection + config/tasks
    5. config/tasks in current directory

    Example:
        >>> template_dir = TemplatePath.find_template_config_directory()
        >>> if template_dir:
        ...     templates = TemplatePath.discover_template_files(template_dir)
        ...     print(f"Found {len(templates)} template files")
    """

    @staticmethod
    def find_template_config_directory() -> Path | None:
        """Find the template configuration directory.

        Returns
        -------
        Path | None
            Path to template directory, or None if not found.

        Notes
        -----
        Search priority:
        1. TASKER_TEMPLATE_PATH environment variable
        2. Test environment: looks for test fixtures
        3. WORKSPACE_PATH/config/tasks
        4. Auto-detect workspace root + config/tasks
        5. ./config/tasks fallback
        """
        # 1. Explicit override via environment variable
        env_path = os.environ.get("TASKER_TEMPLATE_PATH")
        if env_path:
            path = Path(env_path)
            if path.is_dir():
                log_debug(f"Using TASKER_TEMPLATE_PATH: {path}")
                return path
            log_warn(f"TASKER_TEMPLATE_PATH does not exist: {env_path}")

        # 2. Test environment defaults
        if TemplatePath._is_test_environment():
            test_path = TemplatePath._find_test_template_path()
            if test_path:
                log_debug(f"Using test template path: {test_path}")
                return test_path

        # 3. WORKSPACE_PATH environment variable
        workspace_path = os.environ.get("WORKSPACE_PATH")
        if workspace_path:
            config_path = Path(workspace_path) / "config" / "tasks"
            if config_path.is_dir():
                log_debug(f"Using WORKSPACE_PATH config: {config_path}")
                return config_path

        # 4. Auto-detect workspace root
        workspace_root = TemplatePath._detect_workspace_root()
        if workspace_root:
            config_path = workspace_root / "config" / "tasks"
            if config_path.is_dir():
                log_debug(f"Using detected workspace config: {config_path}")
                return config_path

        # 5. Fallback to current directory
        fallback_path = Path.cwd() / "config" / "tasks"
        if fallback_path.is_dir():
            log_debug(f"Using fallback config path: {fallback_path}")
            return fallback_path

        log_debug("No template configuration directory found")
        return None

    @staticmethod
    def discover_template_files(template_dir: Path) -> list[Path]:
        """Discover all YAML template files in a directory.

        Parameters
        ----------
        template_dir : Path
            Directory to search for template files.

        Returns
        -------
        list[Path]
            List of paths to YAML template files.

        Notes
        -----
        Searches recursively for .yaml and .yml files.
        """
        if not template_dir.is_dir():
            return []

        templates: list[Path] = []

        # Recursively find all YAML files
        for pattern in ("**/*.yaml", "**/*.yml"):
            templates.extend(template_dir.glob(pattern))

        # Sort for consistent ordering
        templates.sort()

        log_debug(f"Discovered {len(templates)} template files in {template_dir}")
        return templates

    @staticmethod
    def _is_test_environment() -> bool:
        """Check if running in test environment."""
        return os.environ.get("TASKER_ENV") == "test"

    @staticmethod
    def _find_test_template_path() -> Path | None:
        """Find test template fixtures path.

        Searches up from current directory for the test fixtures location.
        """
        # Try relative to current file (workers/python/python/tasker_core/)
        current_file = Path(__file__).resolve()

        # Navigate up to find tests/fixtures/task_templates/python
        # From: workers/python/python/tasker_core/template_discovery.py
        # To:   tests/fixtures/task_templates/python
        search_paths = [
            # From within workers/python directory
            current_file.parent.parent.parent.parent / "tests" / "fixtures" / "task_templates" / "python",
            # From project root
            current_file.parent.parent.parent.parent.parent / "tests" / "fixtures" / "task_templates" / "python",
        ]

        for path in search_paths:
            if path.is_dir():
                return path

        # Also try workspace detection
        workspace_root = TemplatePath._detect_workspace_root()
        if workspace_root:
            test_path = workspace_root / "tests" / "fixtures" / "task_templates" / "python"
            if test_path.is_dir():
                return test_path

        return None

    @staticmethod
    def _detect_workspace_root() -> Path | None:
        """Detect workspace root by searching for marker files.

        Searches up from current directory looking for common workspace
        markers like Cargo.toml, pyproject.toml, .git, etc.
        """
        current = Path.cwd()

        # Search up to 10 levels
        for _ in range(10):
            for marker in WORKSPACE_MARKERS:
                if (current / marker).exists():
                    log_debug(f"Detected workspace root: {current}")
                    return current

            # Move up one directory
            parent = current.parent
            if parent == current:
                break
            current = parent

        return None


class TemplateParser:
    """Parses YAML templates to extract handler information.

    Extracts handler callable names from task template files, supporting
    both task-level handlers and step-level handlers.

    Example:
        >>> template_file = Path("config/tasks/workflow.yaml")
        >>> handlers = TemplateParser.extract_handler_callables(template_file)
        >>> print(f"Found handlers: {handlers}")
    """

    @staticmethod
    def extract_handler_callables(template_file: Path) -> list[str]:
        """Extract handler callables from a template file.

        Parameters
        ----------
        template_file : Path
            Path to YAML template file.

        Returns
        -------
        list[str]
            List of handler callable strings (e.g., "module.ClassName").

        Notes
        -----
        Extracts from:
        - task_handler.callable
        - steps[].handler.callable
        """
        try:
            with template_file.open() as f:
                data = yaml.safe_load(f)
        except (OSError, yaml.YAMLError) as e:
            log_warn(f"Failed to parse template {template_file}: {e}")
            return []

        if not isinstance(data, dict):
            return []

        return TemplateParser.extract_handlers_from_template(data)

    @staticmethod
    def extract_handlers_from_template(template_data: dict[str, Any]) -> list[str]:
        """Extract handler callables from parsed template data.

        Parameters
        ----------
        template_data : dict[str, Any]
            Parsed YAML template data.

        Returns
        -------
        list[str]
            List of handler callable strings.
        """
        handlers: list[str] = []

        # Extract task_handler.callable
        task_handler = template_data.get("task_handler", {})
        if isinstance(task_handler, dict):
            callable_name = task_handler.get("callable")
            if callable_name and isinstance(callable_name, str):
                handlers.append(callable_name)

        # Extract steps[].handler.callable
        steps = template_data.get("steps", [])
        if isinstance(steps, list):
            for step in steps:
                if not isinstance(step, dict):
                    continue

                handler = step.get("handler", {})
                if isinstance(handler, dict):
                    callable_name = handler.get("callable")
                    if callable_name and isinstance(callable_name, str):
                        handlers.append(callable_name)

        return handlers

    @staticmethod
    def extract_template_metadata(template_file: Path) -> dict[str, Any]:
        """Extract full metadata from a template file.

        Parameters
        ----------
        template_file : Path
            Path to YAML template file.

        Returns
        -------
        dict[str, Any]
            Template metadata including name, namespace, version,
            description, and handlers.
        """
        try:
            with template_file.open() as f:
                data = yaml.safe_load(f)
        except (OSError, yaml.YAMLError) as e:
            log_warn(f"Failed to parse template {template_file}: {e}")
            return {}

        if not isinstance(data, dict):
            return {}

        handlers = TemplateParser.extract_handlers_from_template(data)

        return {
            "name": data.get("name", ""),
            "namespace": data.get("namespace_name", ""),
            "version": data.get("version", ""),
            "description": data.get("description", ""),
            "file_path": str(template_file),
            "handlers": handlers,
        }


class HandlerDiscovery:
    """Coordinator for template-based handler discovery.

    Combines template path finding and parsing to discover all handlers
    defined in task templates.

    Example:
        >>> template_dir = TemplatePath.find_template_config_directory()
        >>> if template_dir:
        ...     handlers = HandlerDiscovery.discover_all_handlers(template_dir)
        ...     for handler in handlers:
        ...         print(f"  - {handler}")
    """

    @staticmethod
    def discover_all_handlers(template_dir: Path) -> list[str]:
        """Discover all unique handler callables from templates.

        Parameters
        ----------
        template_dir : Path
            Directory containing template files.

        Returns
        -------
        list[str]
            Sorted list of unique handler callable strings.
        """
        template_files = TemplatePath.discover_template_files(template_dir)

        handlers: set[str] = set()

        for template_file in template_files:
            file_handlers = TemplateParser.extract_handler_callables(template_file)
            handlers.update(file_handlers)

        result = sorted(handlers)
        log_info(f"Discovered {len(result)} unique handlers from {len(template_files)} templates")
        return result

    @staticmethod
    def discover_template_metadata(template_dir: Path) -> list[dict[str, Any]]:
        """Discover metadata for all templates.

        Parameters
        ----------
        template_dir : Path
            Directory containing template files.

        Returns
        -------
        list[dict[str, Any]]
            List of template metadata dictionaries.
        """
        template_files = TemplatePath.discover_template_files(template_dir)

        metadata: list[dict[str, Any]] = []

        for template_file in template_files:
            template_meta = TemplateParser.extract_template_metadata(template_file)
            if template_meta:
                metadata.append(template_meta)

        return metadata

    @staticmethod
    def discover_handlers_by_namespace(
        template_dir: Path,
    ) -> dict[str, list[str]]:
        """Discover handlers grouped by namespace.

        Parameters
        ----------
        template_dir : Path
            Directory containing template files.

        Returns
        -------
        dict[str, list[str]]
            Dictionary mapping namespace names to handler lists.
        """
        template_files = TemplatePath.discover_template_files(template_dir)

        handlers_by_namespace: dict[str, list[str]] = {}

        for template_file in template_files:
            metadata = TemplateParser.extract_template_metadata(template_file)
            if not metadata:
                continue

            namespace = metadata.get("namespace", "default")
            handlers = metadata.get("handlers", [])

            if namespace not in handlers_by_namespace:
                handlers_by_namespace[namespace] = []

            handlers_by_namespace[namespace].extend(handlers)

        # Remove duplicates and sort within each namespace
        for namespace in handlers_by_namespace:
            handlers_by_namespace[namespace] = sorted(set(handlers_by_namespace[namespace]))

        return handlers_by_namespace


__all__ = [
    "HandlerDiscovery",
    "TemplatePath",
    "TemplateParser",
]
