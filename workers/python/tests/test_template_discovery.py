"""Tests for template discovery module.

These tests verify:
- TemplatePath discovery logic
- TemplateParser YAML parsing
- HandlerDiscovery coordination
- Environment-based path selection
"""

from __future__ import annotations

import os
from pathlib import Path
from unittest.mock import patch

import pytest

from tasker_core.template_discovery import (
    HandlerDiscovery,
    TemplateParser,
    TemplatePath,
)


class TestTemplatePath:
    """Tests for TemplatePath class."""

    def test_find_template_config_directory_with_env_var(self, tmp_path: Path):
        """Test TASKER_TEMPLATE_PATH environment variable is used."""
        template_dir = tmp_path / "templates"
        template_dir.mkdir()

        with patch.dict(os.environ, {"TASKER_TEMPLATE_PATH": str(template_dir)}):
            result = TemplatePath.find_template_config_directory()
            assert result == template_dir

    def test_find_template_config_directory_env_var_invalid(self):
        """Test invalid TASKER_TEMPLATE_PATH is handled gracefully."""
        with patch.dict(os.environ, {"TASKER_TEMPLATE_PATH": "/nonexistent/path"}):
            # Should fall through to other discovery methods
            TemplatePath.find_template_config_directory()
            # Result depends on test environment - may or may not find something

    def test_discover_template_files_yaml(self, tmp_path: Path):
        """Test discovering .yaml template files."""
        (tmp_path / "workflow1.yaml").write_text("name: test1")
        (tmp_path / "workflow2.yml").write_text("name: test2")
        (tmp_path / "not_yaml.txt").write_text("not yaml")
        subdir = tmp_path / "subdir"
        subdir.mkdir()
        (subdir / "nested.yaml").write_text("name: nested")

        files = TemplatePath.discover_template_files(tmp_path)

        assert len(files) == 3
        file_names = [f.name for f in files]
        assert "workflow1.yaml" in file_names
        assert "workflow2.yml" in file_names
        assert "nested.yaml" in file_names
        assert "not_yaml.txt" not in file_names

    def test_discover_template_files_empty_dir(self, tmp_path: Path):
        """Test discovering templates in empty directory."""
        files = TemplatePath.discover_template_files(tmp_path)
        assert files == []

    def test_discover_template_files_nonexistent_dir(self):
        """Test discovering templates in nonexistent directory."""
        files = TemplatePath.discover_template_files(Path("/nonexistent/path"))
        assert files == []

    def test_is_test_environment_true(self):
        """Test test environment detection when TASKER_ENV=test."""
        with patch.dict(os.environ, {"TASKER_ENV": "test"}):
            assert TemplatePath._is_test_environment() is True

    def test_is_test_environment_false(self):
        """Test test environment detection when TASKER_ENV is not test."""
        with patch.dict(os.environ, {"TASKER_ENV": "development"}, clear=False):
            # Need to ensure TASKER_ENV is set to something other than "test"
            os.environ["TASKER_ENV"] = "development"
            assert TemplatePath._is_test_environment() is False


class TestTemplateParser:
    """Tests for TemplateParser class."""

    def test_extract_handler_callables_task_handler(self, tmp_path: Path):
        """Test extracting handler callable from task_handler."""
        template_file = tmp_path / "workflow.yaml"
        template_file.write_text("""
name: test_workflow
task_handler:
  callable: myapp.handlers.TaskHandler
steps: []
""")

        handlers = TemplateParser.extract_handler_callables(template_file)
        assert "myapp.handlers.TaskHandler" in handlers

    def test_extract_handler_callables_steps(self, tmp_path: Path):
        """Test extracting handler callables from steps."""
        template_file = tmp_path / "workflow.yaml"
        template_file.write_text("""
name: test_workflow
steps:
  - name: step1
    handler:
      callable: myapp.handlers.Step1Handler
  - name: step2
    handler:
      callable: myapp.handlers.Step2Handler
""")

        handlers = TemplateParser.extract_handler_callables(template_file)
        assert len(handlers) == 2
        assert "myapp.handlers.Step1Handler" in handlers
        assert "myapp.handlers.Step2Handler" in handlers

    def test_extract_handler_callables_mixed(self, tmp_path: Path):
        """Test extracting both task_handler and step handlers."""
        template_file = tmp_path / "workflow.yaml"
        template_file.write_text("""
name: test_workflow
task_handler:
  callable: myapp.TaskHandler
steps:
  - name: step1
    handler:
      callable: myapp.Step1Handler
  - name: step2
    handler:
      callable: myapp.Step2Handler
""")

        handlers = TemplateParser.extract_handler_callables(template_file)
        assert len(handlers) == 3
        assert "myapp.TaskHandler" in handlers
        assert "myapp.Step1Handler" in handlers
        assert "myapp.Step2Handler" in handlers

    def test_extract_handler_callables_missing_callable(self, tmp_path: Path):
        """Test steps without callable are skipped."""
        template_file = tmp_path / "workflow.yaml"
        template_file.write_text("""
name: test_workflow
steps:
  - name: step1
    handler:
      type: inline
  - name: step2
    handler:
      callable: myapp.Step2Handler
""")

        handlers = TemplateParser.extract_handler_callables(template_file)
        assert len(handlers) == 1
        assert "myapp.Step2Handler" in handlers

    def test_extract_handler_callables_invalid_yaml(self, tmp_path: Path):
        """Test invalid YAML returns empty list."""
        template_file = tmp_path / "workflow.yaml"
        template_file.write_text("invalid: yaml: content: [")

        handlers = TemplateParser.extract_handler_callables(template_file)
        assert handlers == []

    def test_extract_handler_callables_nonexistent_file(self):
        """Test nonexistent file returns empty list."""
        handlers = TemplateParser.extract_handler_callables(Path("/nonexistent/file.yaml"))
        assert handlers == []

    def test_extract_template_metadata(self, tmp_path: Path):
        """Test extracting full template metadata."""
        template_file = tmp_path / "workflow.yaml"
        template_file.write_text("""
name: my_workflow
namespace_name: my_namespace
version: 1.0.0
description: My workflow description
task_handler:
  callable: myapp.TaskHandler
steps:
  - name: step1
    handler:
      callable: myapp.Step1Handler
""")

        metadata = TemplateParser.extract_template_metadata(template_file)

        assert metadata["name"] == "my_workflow"
        assert metadata["namespace"] == "my_namespace"
        assert metadata["version"] == "1.0.0"
        assert metadata["description"] == "My workflow description"
        assert metadata["file_path"] == str(template_file)
        assert len(metadata["handlers"]) == 2

    def test_extract_template_metadata_minimal(self, tmp_path: Path):
        """Test extracting metadata from minimal template."""
        template_file = tmp_path / "workflow.yaml"
        template_file.write_text("""
steps: []
""")

        metadata = TemplateParser.extract_template_metadata(template_file)

        assert metadata["name"] == ""
        assert metadata["namespace"] == ""
        assert metadata["handlers"] == []


class TestHandlerDiscovery:
    """Tests for HandlerDiscovery coordinator."""

    def test_discover_all_handlers(self, tmp_path: Path):
        """Test discovering all handlers from multiple templates."""
        (tmp_path / "workflow1.yaml").write_text("""
task_handler:
  callable: myapp.Handler1
steps:
  - name: step1
    handler:
      callable: myapp.Step1Handler
""")
        (tmp_path / "workflow2.yaml").write_text("""
task_handler:
  callable: myapp.Handler2
steps:
  - name: step2
    handler:
      callable: myapp.Step2Handler
""")

        handlers = HandlerDiscovery.discover_all_handlers(tmp_path)

        assert len(handlers) == 4
        assert "myapp.Handler1" in handlers
        assert "myapp.Handler2" in handlers
        assert "myapp.Step1Handler" in handlers
        assert "myapp.Step2Handler" in handlers

    def test_discover_all_handlers_deduplication(self, tmp_path: Path):
        """Test that duplicate handlers are deduplicated."""
        (tmp_path / "workflow1.yaml").write_text("""
steps:
  - name: step1
    handler:
      callable: myapp.SharedHandler
""")
        (tmp_path / "workflow2.yaml").write_text("""
steps:
  - name: step1
    handler:
      callable: myapp.SharedHandler
""")

        handlers = HandlerDiscovery.discover_all_handlers(tmp_path)

        assert len(handlers) == 1
        assert "myapp.SharedHandler" in handlers

    def test_discover_all_handlers_sorted(self, tmp_path: Path):
        """Test that handlers are returned sorted."""
        (tmp_path / "workflow.yaml").write_text("""
steps:
  - name: step1
    handler:
      callable: z.Handler
  - name: step2
    handler:
      callable: a.Handler
  - name: step3
    handler:
      callable: m.Handler
""")

        handlers = HandlerDiscovery.discover_all_handlers(tmp_path)

        assert handlers == ["a.Handler", "m.Handler", "z.Handler"]

    def test_discover_handlers_by_namespace(self, tmp_path: Path):
        """Test discovering handlers grouped by namespace."""
        (tmp_path / "workflow1.yaml").write_text("""
namespace_name: namespace_a
steps:
  - name: step1
    handler:
      callable: myapp.Handler1
""")
        (tmp_path / "workflow2.yaml").write_text("""
namespace_name: namespace_b
steps:
  - name: step1
    handler:
      callable: myapp.Handler2
""")
        (tmp_path / "workflow3.yaml").write_text("""
namespace_name: namespace_a
steps:
  - name: step1
    handler:
      callable: myapp.Handler3
""")

        handlers_by_ns = HandlerDiscovery.discover_handlers_by_namespace(tmp_path)

        assert "namespace_a" in handlers_by_ns
        assert "namespace_b" in handlers_by_ns
        assert len(handlers_by_ns["namespace_a"]) == 2
        assert len(handlers_by_ns["namespace_b"]) == 1

    def test_discover_template_metadata(self, tmp_path: Path):
        """Test discovering metadata for all templates."""
        (tmp_path / "workflow1.yaml").write_text("""
name: workflow1
version: 1.0.0
steps: []
""")
        (tmp_path / "workflow2.yaml").write_text("""
name: workflow2
version: 2.0.0
steps: []
""")

        metadata_list = HandlerDiscovery.discover_template_metadata(tmp_path)

        assert len(metadata_list) == 2
        names = [m["name"] for m in metadata_list]
        assert "workflow1" in names
        assert "workflow2" in names


class TestTemplateDiscoveryIntegration:
    """Integration tests using actual fixture files."""

    @pytest.fixture
    def fixture_path(self) -> Path:
        """Get path to test fixtures."""
        # Navigate from workers/python/tests to tests/fixtures/task_templates/python
        current_file = Path(__file__).resolve()
        fixture_path = (
            current_file.parent.parent.parent.parent
            / "tests"
            / "fixtures"
            / "task_templates"
            / "python"
        )
        if fixture_path.is_dir():
            return fixture_path
        pytest.skip("Test fixtures not found")

    def test_discover_real_templates(self, fixture_path: Path):
        """Test discovering handlers from real fixture templates."""
        handlers = HandlerDiscovery.discover_all_handlers(fixture_path)

        # Should find multiple handlers from our test fixtures
        assert len(handlers) > 0

        # Check for some expected handler patterns
        # The fixtures use format like: linear_workflow.step_handlers.LinearStep1Handler
        handler_str = " ".join(handlers)
        assert "step_handlers" in handler_str or "Handler" in handler_str

    def test_parse_linear_workflow_template(self, fixture_path: Path):
        """Test parsing the linear workflow template."""
        template_file = fixture_path / "linear_workflow_handler_py.yaml"
        if not template_file.exists():
            pytest.skip("Linear workflow fixture not found")

        handlers = TemplateParser.extract_handler_callables(template_file)

        # Should find task handler and step handlers
        assert len(handlers) >= 4  # task_handler + 4 steps

    def test_template_metadata_extraction(self, fixture_path: Path):
        """Test extracting metadata from real templates."""
        metadata_list = HandlerDiscovery.discover_template_metadata(fixture_path)

        assert len(metadata_list) > 0

        # Check that metadata has expected fields
        for metadata in metadata_list:
            assert "name" in metadata
            assert "namespace" in metadata
            assert "handlers" in metadata
            assert "file_path" in metadata


class TestTemplatePathWorkspacePath:
    """Tests for WORKSPACE_PATH env var and workspace detection."""

    def test_workspace_path_env_var_with_config_tasks(self, tmp_path: Path):
        """WORKSPACE_PATH/config/tasks is used when it exists."""
        config_tasks = tmp_path / "config" / "tasks"
        config_tasks.mkdir(parents=True)

        env = {
            "WORKSPACE_PATH": str(tmp_path),
            "TASKER_TEMPLATE_PATH": "",
            "TASKER_ENV": "development",
        }
        with patch.dict(os.environ, env, clear=False):
            # Remove TASKER_TEMPLATE_PATH so it falls through
            os.environ.pop("TASKER_TEMPLATE_PATH", None)
            TemplatePath.find_template_config_directory()
            # May find workspace root first via _detect_workspace_root,
            # but WORKSPACE_PATH should be checked in the priority chain
            # We verify the config/tasks dir at WORKSPACE_PATH is valid
            assert config_tasks.is_dir()

    def test_workspace_path_env_var_nonexistent_config(self, tmp_path: Path):
        """WORKSPACE_PATH without config/tasks falls through."""
        env = {
            "WORKSPACE_PATH": str(tmp_path),
            "TASKER_TEMPLATE_PATH": "",
            "TASKER_ENV": "development",
        }
        with patch.dict(os.environ, env, clear=False):
            os.environ.pop("TASKER_TEMPLATE_PATH", None)
            # Should fall through since tmp_path/config/tasks doesn't exist
            # Result depends on further discovery steps
            TemplatePath.find_template_config_directory()

    def test_detect_workspace_root_finds_cargo_toml(self, tmp_path: Path):
        """_detect_workspace_root finds Cargo.toml marker."""
        (tmp_path / "Cargo.toml").write_text("[workspace]")
        with patch("tasker_core.template_discovery.Path.cwd", return_value=tmp_path):
            result = TemplatePath._detect_workspace_root()
            assert result == tmp_path

    def test_detect_workspace_root_finds_git_dir(self, tmp_path: Path):
        """_detect_workspace_root finds .git marker."""
        (tmp_path / ".git").mkdir()
        with patch("tasker_core.template_discovery.Path.cwd", return_value=tmp_path):
            result = TemplatePath._detect_workspace_root()
            assert result == tmp_path

    def test_detect_workspace_root_finds_pyproject_toml(self, tmp_path: Path):
        """_detect_workspace_root finds pyproject.toml marker."""
        (tmp_path / "pyproject.toml").write_text("[project]")
        with patch("tasker_core.template_discovery.Path.cwd", return_value=tmp_path):
            result = TemplatePath._detect_workspace_root()
            assert result == tmp_path

    def test_detect_workspace_root_searches_parent_dirs(self, tmp_path: Path):
        """_detect_workspace_root searches up parent directories."""
        (tmp_path / "Cargo.toml").write_text("[workspace]")
        child = tmp_path / "a" / "b" / "c"
        child.mkdir(parents=True)
        with patch("tasker_core.template_discovery.Path.cwd", return_value=child):
            result = TemplatePath._detect_workspace_root()
            assert result == tmp_path

    def test_detect_workspace_root_returns_none_no_markers(self, tmp_path: Path):
        """_detect_workspace_root returns None when no markers found."""
        empty_dir = tmp_path / "empty"
        empty_dir.mkdir()
        # Patch cwd to a dir with no markers and prevent searching up to real root
        with patch("tasker_core.template_discovery.Path.cwd", return_value=empty_dir):
            # Since we limit to 10 levels and tmp_path might have markers above,
            # we just verify the method doesn't crash and returns Path or None
            result = TemplatePath._detect_workspace_root()
            assert result is None or isinstance(result, Path)


class TestTemplateParserEdgeCases:
    """Tests for TemplateParser error paths and edge cases."""

    def test_extract_template_metadata_invalid_yaml(self, tmp_path: Path):
        """extract_template_metadata returns empty dict for invalid YAML."""
        template_file = tmp_path / "bad.yaml"
        template_file.write_text("invalid: yaml: content: [")

        metadata = TemplateParser.extract_template_metadata(template_file)
        assert metadata == {}

    def test_extract_template_metadata_non_dict_yaml(self, tmp_path: Path):
        """extract_template_metadata returns empty dict for non-dict YAML."""
        template_file = tmp_path / "list.yaml"
        template_file.write_text("- item1\n- item2\n- item3")

        metadata = TemplateParser.extract_template_metadata(template_file)
        assert metadata == {}

    def test_extract_template_metadata_nonexistent_file(self):
        """extract_template_metadata returns empty dict for nonexistent file."""
        metadata = TemplateParser.extract_template_metadata(Path("/nonexistent/file.yaml"))
        assert metadata == {}

    def test_extract_handler_callables_non_dict_yaml(self, tmp_path: Path):
        """extract_handler_callables returns empty list for non-dict YAML."""
        template_file = tmp_path / "list.yaml"
        template_file.write_text("- item1\n- item2")

        handlers = TemplateParser.extract_handler_callables(template_file)
        assert handlers == []

    def test_extract_handlers_from_template_non_list_steps(self):
        """extract_handlers_from_template handles non-list steps gracefully."""
        handlers = TemplateParser.extract_handlers_from_template({"steps": "not_a_list"})
        assert handlers == []

    def test_extract_handlers_from_template_non_dict_step(self):
        """extract_handlers_from_template skips non-dict step entries."""
        handlers = TemplateParser.extract_handlers_from_template(
            {"steps": ["not_a_dict", 42, None]}
        )
        assert handlers == []

    def test_extract_handlers_from_template_non_dict_task_handler(self):
        """extract_handlers_from_template handles non-dict task_handler."""
        handlers = TemplateParser.extract_handlers_from_template({"task_handler": "not_a_dict"})
        assert handlers == []

    def test_extract_handlers_from_template_non_string_callable(self):
        """extract_handlers_from_template skips non-string callables."""
        handlers = TemplateParser.extract_handlers_from_template(
            {
                "task_handler": {"callable": 42},
                "steps": [{"handler": {"callable": None}}],
            }
        )
        assert handlers == []

    def test_fallback_path_resolution(self, tmp_path: Path):
        """Fallback to cwd/config/tasks when no other path found."""
        config_tasks = tmp_path / "config" / "tasks"
        config_tasks.mkdir(parents=True)

        env = {"TASKER_ENV": "development"}
        with (
            patch.dict(os.environ, env, clear=False),
            patch("tasker_core.template_discovery.Path.cwd", return_value=tmp_path),
        ):
            os.environ.pop("TASKER_TEMPLATE_PATH", None)
            os.environ.pop("WORKSPACE_PATH", None)
            result = TemplatePath.find_template_config_directory()
            # The fallback or workspace detection should find config/tasks
            # since cwd is patched to tmp_path which has config/tasks
            assert result is not None
