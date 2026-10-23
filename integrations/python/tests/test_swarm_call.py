"""Tests for the AetherShell code the SDK generates for swarms.

These check the generated string, not a running `ae`: the eval call is mocked
and the code it receives is inspected. The shape asserted here is the one the
`swarm` builtin accepts (src/builtins.rs, parse_agent_call and agent_config):
a config record {goal, tools, max_steps, dry_run}.
"""

import asyncio
from unittest.mock import MagicMock

import pytest

from aethershell import Agent, AetherRuntime, Swarm, _build_swarm_code


def _runtime_returning(value):
    runtime = MagicMock(spec=AetherRuntime)
    runtime.eval.return_value = value
    return runtime


def _run(swarm, goal):
    return asyncio.run(swarm.run(goal))


class TestGeneratedSwarmCall:
    def test_record_form_with_goal_tools_and_steps(self):
        planner = Agent(name="planner", tools=["ls", "cat"])
        coder = Agent(name="coder", tools=["cat", "write"])

        code = _build_swarm_code([planner, coder], "Refactor utils", "round_robin", 10)

        assert code == (
            'swarm({goal: "Refactor utils", tools: ["ls", "cat", "write"], max_steps: 10})'
        )

    def test_old_agent_shape_is_gone(self):
        agent = Agent(name="planner", tools=["ls"])

        code = _build_swarm_code([agent], "goal", "round_robin", 10)

        assert "policy" not in code
        assert "max_iters" not in code
        assert "name:" not in code
        assert "swarm([" not in code

    def test_tools_are_the_deduplicated_union_in_first_seen_order(self):
        a = Agent(name="a", tools=["cat", "ls"])
        b = Agent(name="b", tools=["grep", "cat"])
        c = Agent(name="c")

        code = _build_swarm_code([a, b, c], "goal", "round_robin", 3)

        assert 'tools: ["cat", "ls", "grep"]' in code

    def test_no_tools_gives_empty_array(self):
        code = _build_swarm_code([Agent(name="a")], "goal", "round_robin", 10)

        assert "tools: []" in code

    def test_max_iterations_maps_to_max_steps(self):
        code = _build_swarm_code([Agent(name="a", tools=["ls"])], "goal", "round_robin", 37)

        assert code.endswith("max_steps: 37})")
        assert "max_iters" not in code

    @pytest.mark.parametrize("bad", [0, -1, 1001, 2.5, "10", True])
    def test_max_iterations_out_of_range_or_wrong_type_is_refused(self, bad):
        with pytest.raises(ValueError, match="max_iterations"):
            _build_swarm_code([Agent(name="a")], "goal", "round_robin", bad)

    def test_bounds_are_accepted(self):
        assert "max_steps: 1})" in _build_swarm_code([Agent(name="a")], "g", "round_robin", 1)
        assert "max_steps: 1000})" in _build_swarm_code(
            [Agent(name="a")], "g", "round_robin", 1000
        )


class TestGoalEscaping:
    def test_double_quote_in_goal_is_escaped(self):
        code = _build_swarm_code([Agent(name="a")], 'Say "hi" now', "round_robin", 10)

        assert 'goal: "Say \\"hi\\" now"' in code

    def test_backslash_is_escaped_before_quote(self):
        code = _build_swarm_code([Agent(name="a")], 'C:\\tmp "x"', "round_robin", 10)

        # A backslash becomes \\ and a quote becomes \" so the literal ends
        # where the goal ends.
        assert 'goal: "C:\\\\tmp \\"x\\""' in code

    def test_newline_and_tab_are_escaped(self):
        code = _build_swarm_code([Agent(name="a")], "line1\nline2\tend", "round_robin", 10)

        assert "\n" not in code
        assert "\t" not in code
        assert 'goal: "line1\\nline2\\tend"' in code

    def test_dollar_brace_is_refused_rather_than_interpolated(self):
        with pytest.raises(ValueError, match=r"\$\{"):
            _build_swarm_code([Agent(name="a")], "echo ${HOME}", "round_robin", 10)

    def test_dollar_without_brace_is_fine(self):
        code = _build_swarm_code([Agent(name="a")], "price is $5", "round_robin", 10)

        assert 'goal: "price is $5"' in code

    def test_tool_names_are_escaped_too(self):
        code = _build_swarm_code([Agent(name="a", tools=['we"ird'])], "g", "round_robin", 10)

        assert 'tools: ["we\\"ird"]' in code

    def test_non_string_goal_is_rejected(self):
        with pytest.raises(TypeError):
            _build_swarm_code([Agent(name="a")], 42, "round_robin", 10)


class TestUnsupportedOptions:
    def test_router_policy_raises(self):
        with pytest.raises(ValueError, match="policy"):
            _build_swarm_code([Agent(name="a")], "goal", "router", 10)

    def test_per_agent_model_raises(self):
        agent = Agent(name="coder", model="openai:gpt-4o", tools=["cat"])

        with pytest.raises(ValueError, match="model"):
            _build_swarm_code([agent], "goal", "round_robin", 10)

    def test_per_agent_max_steps_raises(self):
        agent = Agent(name="coder", max_steps=5)

        with pytest.raises(ValueError, match="max_steps"):
            _build_swarm_code([agent], "goal", "round_robin", 10)

    def test_default_model_and_steps_are_accepted(self):
        agent = Agent(name="coder", model="openai:gpt-4o-mini", max_steps=10)

        code = _build_swarm_code([agent], "goal", "round_robin", 10)

        assert code.startswith("swarm({")

    def test_run_raises_before_calling_the_runtime(self):
        runtime = _runtime_returning("unused")
        swarm = Swarm(
            agents=[Agent(name="a", model="ollama:llama3")],
            runtime=runtime,
        )

        with pytest.raises(ValueError, match="model"):
            _run(swarm, "goal")
        runtime.eval.assert_not_called()


class TestSwarmRun:
    def test_run_evaluates_the_generated_call(self):
        runtime = _runtime_returning("combined answer")
        swarm = Swarm(
            agents=[Agent(name="a", tools=["ls"]), Agent(name="b", tools=["cat"])],
            max_iterations=4,
            runtime=runtime,
        )

        _run(swarm, 'Find "TODO" items')

        runtime.eval.assert_called_once_with(
            'swarm({goal: "Find \\"TODO\\" items", tools: ["ls", "cat"], max_steps: 4})'
        )

    def test_string_answer_becomes_swarm_result(self):
        swarm = Swarm(agents=[Agent(name="a")], runtime=_runtime_returning("done"))

        result = _run(swarm, "goal")

        assert result.success is True
        assert result.result == "done"
        assert result.blackboard == {}
        assert result.iterations == 0

    def test_record_answer_is_still_mapped(self):
        record = {"success": True, "result": "r", "blackboard": {"k": 1}, "iterations": 2}
        swarm = Swarm(agents=[Agent(name="a")], runtime=_runtime_returning(record))

        result = _run(swarm, "goal")

        assert result.success is True
        assert result.blackboard == {"k": 1}
        assert result.iterations == 2
