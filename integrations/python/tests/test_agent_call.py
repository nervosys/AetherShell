"""Tests for the AetherShell code the SDK generates for a single agent.

The eval call is mocked and the code it receives is inspected. The shape
asserted is the one the `agent` builtin accepts (src/builtins.rs,
parse_agent_call and agent_config): a config record {goal, tools, max_steps, dry_run}.
"""

import asyncio
from unittest.mock import MagicMock

import pytest

from aethershell import Agent, AetherRuntime, _build_agent_code


def _runtime_returning(value):
    runtime = MagicMock(spec=AetherRuntime)
    runtime.eval.return_value = value
    return runtime


def _run(agent, goal):
    return asyncio.run(agent.run(goal))


class TestGeneratedAgentCall:
    def test_record_form_with_goal_tools_and_steps(self):
        code = _build_agent_code("Refactor utils", "openai:gpt-4o-mini", ["ls", "cat"], 10)

        assert code == 'agent({goal: "Refactor utils", tools: ["ls", "cat"], max_steps: 10})'

    def test_old_positional_shape_is_gone(self):
        code = _build_agent_code("goal", "openai:gpt-4o-mini", ["ls"], 10)

        assert not code.startswith('agent("')
        assert "openai:gpt-4o-mini" not in code

    def test_empty_tools_gives_empty_array(self):
        code = _build_agent_code("goal", "openai:gpt-4o-mini", [], 10)

        assert "tools: []" in code

    def test_tool_order_is_kept(self):
        code = _build_agent_code("goal", "openai:gpt-4o-mini", ["cat", "ls", "cat"], 3)

        assert 'tools: ["cat", "ls", "cat"]' in code

    @pytest.mark.parametrize("bad", [0, -1, 1001, 2.5, "10", True])
    def test_max_steps_out_of_range_or_wrong_type_is_refused(self, bad):
        with pytest.raises(ValueError, match="max_steps"):
            _build_agent_code("goal", "openai:gpt-4o-mini", [], bad)

    def test_bounds_are_accepted(self):
        assert "max_steps: 1})" in _build_agent_code("g", "openai:gpt-4o-mini", [], 1)
        assert "max_steps: 1000})" in _build_agent_code("g", "openai:gpt-4o-mini", [], 1000)


class TestGoalEscaping:
    def test_double_quote_in_goal_is_escaped(self):
        code = _build_agent_code('Say "hi" now', "openai:gpt-4o-mini", [], 10)

        assert 'goal: "Say \\"hi\\" now"' in code

    def test_backslash_is_escaped_before_quote(self):
        code = _build_agent_code('C:\\tmp "x"', "openai:gpt-4o-mini", [], 10)

        assert 'goal: "C:\\\\tmp \\"x\\""' in code

    def test_dollar_brace_is_refused(self):
        with pytest.raises(ValueError, match=r"\$\{"):
            _build_agent_code("echo ${HOME}", "openai:gpt-4o-mini", [], 10)

    def test_tool_names_are_escaped(self):
        code = _build_agent_code("g", "openai:gpt-4o-mini", ['we"ird'], 10)

        assert 'tools: ["we\\"ird"]' in code


class TestUnsupportedOptions:
    def test_non_default_model_raises(self):
        with pytest.raises(ValueError, match="model"):
            _build_agent_code("goal", "ollama:llama3", [], 10)

    def test_run_raises_before_calling_the_runtime(self):
        runtime = _runtime_returning("unused")
        agent = Agent(name="a", model="openai:gpt-4o", runtime=runtime)

        with pytest.raises(ValueError, match="model"):
            _run(agent, "goal")
        runtime.eval.assert_not_called()

    def test_run_refuses_max_steps_over_bound(self):
        runtime = _runtime_returning("unused")
        agent = Agent(name="a", max_steps=5000, runtime=runtime)

        with pytest.raises(ValueError, match="max_steps"):
            _run(agent, "goal")
        runtime.eval.assert_not_called()


class TestAgentRun:
    def test_run_evaluates_the_generated_call(self):
        runtime = _runtime_returning("final answer")
        agent = Agent(name="a", tools=["ls", "cat"], max_steps=6, runtime=runtime)

        _run(agent, 'Find "TODO" items')

        runtime.eval.assert_called_once_with(
            'agent({goal: "Find \\"TODO\\" items", tools: ["ls", "cat"], max_steps: 6})'
        )

    def test_string_answer_becomes_agent_result(self):
        agent = Agent(name="a", runtime=_runtime_returning("done"))

        result = _run(agent, "goal")

        assert result.success is True
        assert result.result == "done"
        assert result.trace == []
        assert result.steps_taken == 0

    def test_record_answer_is_still_mapped(self):
        record = {"success": True, "result": "r", "trace": [{"s": 1}], "steps": 3}
        agent = Agent(name="a", runtime=_runtime_returning(record))

        result = _run(agent, "goal")

        assert result.success is True
        assert result.trace == [{"s": 1}]
        assert result.steps_taken == 3
