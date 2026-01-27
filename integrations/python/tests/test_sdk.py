"""Tests for AetherShell Python SDK"""

import pytest
import json
from unittest.mock import patch, MagicMock
import subprocess

from aethershell import (
    AetherRuntime,
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


class TestAetherRuntime:
    """Tests for AetherRuntime class"""
    
    @patch('aethershell.subprocess.run')
    @patch('aethershell.os.path.isfile', return_value=True)
    def test_eval_basic(self, mock_isfile, mock_run):
        """Test basic evaluation"""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='[2, 4, 6]',
            stderr='',
        )
        
        runtime = AetherRuntime(ae_path="/usr/bin/ae")
        result = runtime.eval('[1, 2, 3] | map(fn(x) => x * 2)')
        
        assert result == [2, 4, 6]
        mock_run.assert_called_once()
    
    @patch('aethershell.subprocess.run')
    @patch('aethershell.os.path.isfile', return_value=True)
    def test_eval_json_result(self, mock_isfile, mock_run):
        """Test evaluation returning JSON"""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='{"name": "test", "value": 42}',
            stderr='',
        )
        
        runtime = AetherRuntime(ae_path="/usr/bin/ae")
        result = runtime.eval('{"name": "test", "value": 42}')
        
        assert result == {"name": "test", "value": 42}
    
    @patch('aethershell.subprocess.run')
    @patch('aethershell.os.path.isfile', return_value=True)
    def test_eval_error(self, mock_isfile, mock_run):
        """Test evaluation with error"""
        mock_run.return_value = MagicMock(
            returncode=1,
            stdout='',
            stderr='Parse error: unexpected token',
        )
        
        runtime = AetherRuntime(ae_path="/usr/bin/ae")
        
        with pytest.raises(RuntimeError, match="AetherShell error"):
            runtime.eval('invalid code {{{{')
    
    def test_ae_not_found(self):
        """Test error when ae binary not found"""
        with patch('aethershell.os.environ.get', return_value=''):
            with patch('aethershell.os.path.isfile', return_value=False):
                with pytest.raises(RuntimeError, match="Could not find 'ae'"):
                    AetherRuntime()


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
        assert "filter(fn(x) => x > 2)" in code
    
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
        assert "filter" in code
        assert "sort" in code
    
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
    
    @patch('aethershell.AetherRuntime')
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


# Integration tests (require ae binary)
@pytest.mark.integration
class TestIntegration:
    """Integration tests that require the ae binary"""
    
    def test_eval_simple(self):
        """Test simple evaluation"""
        runtime = AetherRuntime()
        result = runtime.eval("1 + 2")
        assert result == 3
    
    def test_eval_array(self):
        """Test array evaluation"""
        runtime = AetherRuntime()
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


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
