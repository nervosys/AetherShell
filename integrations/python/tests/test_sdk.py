"""Tests for IronShell Python SDK"""

import importlib.metadata
import inspect
import re
from pathlib import Path

import pytest
import json
from unittest.mock import patch, MagicMock
import subprocess

import ironshell
from ironshell import (
    IronShellRuntime,
    Agent,
    AgentConfig,
    AgentResult,
    Swarm,
    SwarmConfig,
    SwarmResult,
    PipelineBuilder,
    evaluate,
    pipeline,
    NotificationLevel,
    A2UIEvent,
)


class TestIronShellRuntime:
    """Tests for IronShellRuntime class"""
    
    @patch('ironshell.subprocess.run')
    @patch('ironshell.os.path.isfile', return_value=True)
    def test_eval_basic(self, mock_isfile, mock_run):
        """Test basic evaluation"""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='[2, 4, 6]',
            stderr='',
        )
        
        runtime = IronShellRuntime(ish_path="/usr/bin/ae")
        result = runtime.eval('[1, 2, 3] | map(fn(x) => x * 2)')
        
        assert result == [2, 4, 6]
        mock_run.assert_called_once()
    
    @patch('ironshell.subprocess.run')
    @patch('ironshell.os.path.isfile', return_value=True)
    def test_eval_json_result(self, mock_isfile, mock_run):
        """Test evaluation returning JSON"""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='{"name": "test", "value": 42}',
            stderr='',
        )
        
        runtime = IronShellRuntime(ish_path="/usr/bin/ae")
        result = runtime.eval('{"name": "test", "value": 42}')
        
        assert result == {"name": "test", "value": 42}
    
    @patch('ironshell.subprocess.run')
    @patch('ironshell.os.path.isfile', return_value=True)
    def test_eval_error(self, mock_isfile, mock_run):
        """Test evaluation with error"""
        mock_run.return_value = MagicMock(
            returncode=1,
            stdout='',
            stderr='Parse error: unexpected token',
        )
        
        runtime = IronShellRuntime(ish_path="/usr/bin/ae")
        
        with pytest.raises(RuntimeError, match="IronShell error"):
            runtime.eval('invalid code {{{{')
    
    def test_ae_not_found(self):
        """Test error when ish binary not found"""
        with patch('ironshell.os.environ.get', return_value=''):
            with patch('ironshell.os.path.isfile', return_value=False):
                with pytest.raises(RuntimeError, match="Could not find 'ish'"):
                    IronShellRuntime()


class TestAgent:
    """Tests for Agent class"""
    
    def test_agent_creation(self):
        """Test agent creation"""
        agent = Agent(
            name="test_agent",
            model="openai:gpt-4o",
            tools=["http_get", "search"],
            max_steps=5,
        )
        
        assert agent.name == "test_agent"
        assert agent.model == "openai:gpt-4o"
        assert agent.tools == ["http_get", "search"]
        assert agent.max_steps == 5
    
    def test_agent_repr(self):
        """Test agent string representation"""
        agent = Agent(name="test", model="openai:gpt-4o", tools=["tool1"])
        repr_str = repr(agent)
        
        assert "test" in repr_str
        assert "openai:gpt-4o" in repr_str
        assert "tool1" in repr_str


class TestSwarm:
    """Tests for Swarm class"""
    
    def test_swarm_creation(self):
        """Test swarm creation"""
        agent1 = Agent(name="agent1", model="openai:gpt-4o")
        agent2 = Agent(name="agent2", model="openai:gpt-4o")
        
        swarm = Swarm(
            agents=[agent1, agent2],
            policy="router",
            max_iterations=5,
        )
        
        assert len(swarm.agents) == 2
        assert swarm.policy == "router"
        assert swarm.max_iterations == 5


class TestPipelineBuilder:
    """Tests for PipelineBuilder class"""
    
    def test_pipeline_map(self):
        """Test pipeline map operation"""
        builder = PipelineBuilder([1, 2, 3])
        builder.map("fn(x) => x * 2")
        
        code = builder.to_code()
        assert "map(fn(x) => x * 2)" in code
    
    def test_pipeline_filter(self):
        """Test pipeline filter operation"""
        builder = PipelineBuilder([1, 2, 3, 4, 5])
        builder.filter("fn(x) => x > 2")
        
        code = builder.to_code()
        # The shell has no `filter` builtin; its name is `where`.
        assert "where(fn(x) => x > 2)" in code
        assert "filter(" not in code
    
    def test_pipeline_chaining(self):
        """Test pipeline operation chaining"""
        code = (
            pipeline([1, 2, 3, 4, 5])
            .map("fn(x) => x * 2")
            .filter("fn(x) => x > 4")
            .sort()
            .to_code()
        )
        
        assert "map" in code
        assert "where" in code
        assert "sort" in code

    def test_pipeline_sort_with_key(self):
        """A key function sorts with sort_by: sort() takes no argument"""
        code = pipeline([3, 1, 2]).sort("fn(x) => 0 - x").to_code()
        assert "sort_by(fn(x) => 0 - x)" in code
    
    def test_pipeline_reduce(self):
        """Test pipeline reduce operation"""
        builder = PipelineBuilder([1, 2, 3])
        builder.reduce("fn(acc, x) => acc + x", 0)
        
        code = builder.to_code()
        assert "reduce" in code
        assert "0" in code
    
    def test_pipeline_take_skip(self):
        """Test pipeline take and skip operations"""
        code = (
            pipeline([1, 2, 3, 4, 5])
            .skip(2)
            .take(2)
            .to_code()
        )
        
        assert "slice(2)" in code
        assert "slice(0, 2)" in code


class TestConvenienceFunctions:
    """Tests for convenience functions"""
    
    @patch('ironshell.IronShellRuntime')
    def test_evaluate(self, mock_runtime_class):
        """Test evaluate convenience function"""
        mock_runtime = MagicMock()
        mock_runtime.eval.return_value = 42
        mock_runtime_class.return_value = mock_runtime
        
        result = evaluate("21 * 2")
        
        assert result == 42
        mock_runtime.eval.assert_called_once_with("21 * 2")


class TestDataClasses:
    """Tests for data classes"""
    
    def test_agent_config(self):
        """Test AgentConfig dataclass"""
        config = AgentConfig(
            name="test",
            model="openai:gpt-4o",
            tools=["tool1"],
            max_steps=5,
            dry_run=True,
        )
        
        assert config.name == "test"
        assert config.dry_run is True
    
    def test_agent_result(self):
        """Test AgentResult dataclass"""
        result = AgentResult(
            success=True,
            result={"answer": "42"},
            trace=[{"step": 1}],
            steps_taken=1,
        )
        
        assert result.success is True
        assert result.steps_taken == 1
    
    def test_swarm_result(self):
        """Test SwarmResult dataclass"""
        result = SwarmResult(
            success=True,
            result="completed",
            blackboard={"key": "value"},
            iterations=3,
        )
        
        assert result.success is True
        assert result.blackboard == {"key": "value"}
    
    def test_a2ui_event(self):
        """Test A2UIEvent dataclass"""
        event = A2UIEvent(
            id="evt_123",
            timestamp="2024-01-01T00:00:00Z",
            priority="high",
            event_type="notify",
            data={"message": "test"},
        )
        
        assert event.id == "evt_123"
        assert event.event_type == "notify"


class TestNotificationLevel:
    """Tests for NotificationLevel enum"""
    
    def test_levels(self):
        """Test notification level values"""
        assert NotificationLevel.INFO.value == "info"
        assert NotificationLevel.SUCCESS.value == "success"
        assert NotificationLevel.WARNING.value == "warning"
        assert NotificationLevel.ERROR.value == "error"


# Integration tests (require ish binary)
@pytest.mark.integration
class TestIntegration:
    """Integration tests that require the ish binary"""
    
    def test_eval_simple(self):
        """Test simple evaluation"""
        runtime = IronShellRuntime()
        result = runtime.eval("1 + 2")
        assert result == 3
    
    def test_eval_array(self):
        """Test array evaluation"""
        runtime = IronShellRuntime()
        result = runtime.eval("[1, 2, 3]")
        assert result == [1, 2, 3]
    
    def test_pipeline_execution(self):
        """Test pipeline execution"""
        result = (
            pipeline([1, 2, 3, 4, 5])
            .map("fn(x) => x * 2")
            .filter("fn(x) => x > 4")
            .run()
        )
        
        assert sorted(result) == [6, 8, 10]


def _lexer_decode(literal: str) -> str:
    """Decode a double-quoted literal the way src/parser.rs does.

    Only \\n, \\r, \\t, \\\\ and \\" are special; any other escaped character
    stands for itself, so a \\u escape decodes to a plain u.
    """
    assert literal.startswith('"')
    out = []
    i = 1
    while True:
        ch = literal[i]
        if ch == '"':
            assert i == len(literal) - 1, "text after the closing quote"
            return "".join(out)
        if ch == "\\":
            i += 1
            esc = literal[i]
            out.append({"n": "\n", "r": "\r", "t": "\t"}.get(esc, esc))
        else:
            out.append(ch)
        i += 1


def _pyproject_version() -> str:
    text = (Path(__file__).resolve().parents[1] / "pyproject.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    assert match, "pyproject.toml has no version"
    return match.group(1)


class TestPipelineLiterals:
    """PipelineBuilder encodes data as IronShell literals, not JSON"""

    def test_non_ascii_string_is_written_as_itself(self):
        code = pipeline("café 🚀").map("fn(s) => s").to_code()

        assert code == '"café 🚀" | map(fn(s) => s)'
        assert "\\u" not in code

    def test_non_ascii_inside_a_list_and_reduce_initial(self):
        builder = pipeline(["naïve", "日本"]).reduce("fn(a, b) => a + b", "é")
        code = builder.to_code()

        assert code == '["naïve", "日本"] | reduce(fn(a, b) => a + b, "é")'
        assert "\\u" not in code

    @pytest.mark.parametrize(
        "text",
        [
            'say "hi"',
            "back\\slash",
            "line one\nline two",
            "tab\there\r\n",
            "emoji 🚀 and café",
            'mix "a\\b"\n\t🚀',
        ],
    )
    def test_string_data_round_trips_through_the_lexer_rules(self, text):
        code = pipeline(text).to_code()

        assert code.endswith(" | ")
        assert _lexer_decode(code[: -len(" | ")]) == text

    def test_reduce_initial_round_trips(self):
        code = pipeline([1]).reduce("fn(a, b) => a", 'q"\\\n🚀').to_code()

        assert _lexer_decode(code.split("reduce(fn(a, b) => a, ")[1][:-1]) == 'q"\\\n🚀'

    def test_scalars_and_containers_keep_the_old_spacing(self):
        assert pipeline([1, 2, 3]).to_code() == "[1, 2, 3] | "
        assert pipeline(None).to_code() == "null | "
        assert pipeline([True, False]).to_code() == "[true, false] | "
        assert pipeline(1.5).to_code() == "1.5 | "

    def test_record_keys_must_be_identifiers(self):
        assert pipeline({"name": "ae"}).to_code() == '{name: "ae"} | '
        with pytest.raises(ValueError, match="identifier"):
            pipeline({"not a name": 1}).to_code()

    @pytest.mark.parametrize("bad", ["nul\x00byte", "bell\x07", "esc\x1b["])
    def test_control_characters_are_refused_not_emitted(self, bad):
        with pytest.raises(ValueError, match="control character"):
            pipeline(bad).to_code()

    def test_lone_surrogate_is_refused(self):
        with pytest.raises(ValueError, match="surrogate"):
            pipeline("bad \ud800").to_code()

    def test_non_finite_float_is_refused(self):
        with pytest.raises(ValueError, match="no IronShell literal"):
            pipeline(float("nan")).to_code()

    def test_unknown_type_is_refused(self):
        with pytest.raises(TypeError):
            pipeline(object()).to_code()


class TestPackageSurface:
    """__all__ and __version__ describe the package that exists"""

    @pytest.mark.parametrize("name", ironshell.__all__)
    def test_every_all_name_resolves_on_the_package(self, name):
        assert hasattr(ironshell, name)

    def test_all_has_no_duplicates(self):
        assert len(ironshell.__all__) == len(set(ironshell.__all__))

    def test_all_lists_every_public_class_and_function_defined_here(self):
        defined = {
            name
            for name, obj in vars(ironshell).items()
            if not name.startswith("_")
            and (inspect.isclass(obj) or inspect.isfunction(obj))
            and obj.__module__ == "ironshell"
        }

        assert defined <= set(ironshell.__all__)

    def test_version_matches_installed_metadata_or_pyproject(self):
        try:
            expected = importlib.metadata.version("ironshell")
        except importlib.metadata.PackageNotFoundError:
            expected = _pyproject_version()

        assert ironshell.__version__ == expected


if __name__ == "__main__":
    pytest.main([__file__, "-v"])


class TestWorkflowDataIsNotCode:
    """Workflow data was inlined with json.dumps, so a ``${...}`` in a value
    would have run as code when the evaluator read the string literal."""

    def test_interpolation_in_data_is_refused(self):
        from ironshell import _ironshell_value

        with pytest.raises(ValueError):
            _ironshell_value({"name": "${rm('x')}"})

    def test_langchain_does_not_inline_data_with_json_dumps(self):
        import ironshell.langchain as lc
        from pathlib import Path

        source = Path(lc.__file__).read_text(encoding="utf-8")
        assert "let x = {json.dumps" not in source
        assert "let a = {json.dumps" not in source
