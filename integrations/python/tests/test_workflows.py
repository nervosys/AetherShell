"""
Tests for AetherShell Python SDK - Workflows
"""

import asyncio
import pytest
from unittest.mock import MagicMock, patch

from aethershell.workflows import (
    MapReduceWorkflow,
    SagaWorkflow,
    SagaStep,
    FanOutWorkflow,
    PipelineWorkflow,
    CircuitBreaker,
    CircuitState,
    WorkflowResult,
)


class TestCircuitBreaker:
    """Tests for CircuitBreaker"""
    
    def test_initial_state_closed(self):
        """Circuit starts in closed state"""
        cb = CircuitBreaker(name="test")
        assert cb.state == CircuitState.CLOSED
    
    def test_successful_calls_stay_closed(self):
        """Successful calls keep circuit closed"""
        cb = CircuitBreaker(name="test", failure_threshold=3)
        
        for _ in range(10):
            result = cb.call(lambda: 42)
            assert result == 42
        
        assert cb.state == CircuitState.CLOSED
    
    def test_failures_open_circuit(self):
        """Enough failures open the circuit"""
        cb = CircuitBreaker(name="test", failure_threshold=3)
        
        def failing_fn():
            raise ValueError("fail")
        
        for i in range(3):
            with pytest.raises(ValueError):
                cb.call(failing_fn)
        
        assert cb.state == CircuitState.OPEN
    
    def test_open_circuit_rejects_calls(self):
        """Open circuit rejects calls immediately"""
        cb = CircuitBreaker(name="test", failure_threshold=1)
        
        with pytest.raises(ValueError):
            cb.call(lambda: (_ for _ in ()).throw(ValueError("fail")))
        
        with pytest.raises(RuntimeError, match="is open"):
            cb.call(lambda: 42)
    
    def test_reset_closes_circuit(self):
        """Manual reset closes the circuit"""
        cb = CircuitBreaker(name="test", failure_threshold=1)
        cb._state = CircuitState.OPEN
        
        cb.reset()
        
        assert cb.state == CircuitState.CLOSED
        assert cb._failures == 0


class TestPipelineWorkflow:
    """Tests for PipelineWorkflow"""
    
    @pytest.mark.asyncio
    async def test_empty_pipeline(self):
        """Empty pipeline returns input unchanged"""
        workflow = PipelineWorkflow("test")
        result = await workflow.run(42)
        
        assert result.success
        assert result.result == 42
        assert result.steps_completed == 0
    
    @pytest.mark.asyncio
    async def test_single_stage(self):
        """Single stage pipeline transforms data"""
        workflow = PipelineWorkflow("test", stages=[lambda x: x * 2])
        result = await workflow.run(21)
        
        assert result.success
        assert result.result == 42
        assert result.steps_completed == 1
    
    @pytest.mark.asyncio
    async def test_multi_stage_pipeline(self):
        """Multi-stage pipeline chains transformations"""
        workflow = PipelineWorkflow("test")
        workflow.add_stage(lambda x: x + 1)
        workflow.add_stage(lambda x: x * 2)
        workflow.add_stage(lambda x: x - 10)
        
        result = await workflow.run(10)
        
        assert result.success
        assert result.result == 12  # (10+1)*2 - 10 = 12
        assert result.steps_completed == 3
    
    @pytest.mark.asyncio
    async def test_pipeline_stage_failure(self):
        """Failed stage stops pipeline and reports error"""
        def failing_stage(x):
            raise ValueError("Stage failed")
        
        workflow = PipelineWorkflow("test", stages=[
            lambda x: x + 1,
            failing_stage,
            lambda x: x * 2,
        ])
        
        result = await workflow.run(10)
        
        assert not result.success
        assert result.steps_completed == 1
        assert len(result.errors) == 1
        assert "Stage 1 failed" in result.errors[0]


class TestMapReduceWorkflow:
    """Tests for MapReduceWorkflow"""
    
    @pytest.mark.asyncio
    async def test_sum_numbers(self):
        """MapReduce sums numbers correctly"""
        workflow = MapReduceWorkflow(
            name="sum",
            map_fn=lambda x: x * 2,
            reduce_fn=lambda a, b: a + b,
            chunk_size=2,
        )
        
        result = await workflow.run([1, 2, 3, 4, 5])
        
        assert result.success
        assert result.result == 30  # (1+2+3+4+5) * 2
    
    @pytest.mark.asyncio
    async def test_empty_input(self):
        """MapReduce handles empty input"""
        workflow = MapReduceWorkflow(
            name="test",
            map_fn=lambda x: x,
            reduce_fn=lambda a, b: a + b,
        )
        
        result = await workflow.run([])
        
        assert result.success
        assert result.result is None
    
    @pytest.mark.asyncio
    async def test_single_element(self):
        """MapReduce handles single element"""
        workflow = MapReduceWorkflow(
            name="test",
            map_fn=lambda x: x * 2,
            reduce_fn=lambda a, b: a + b,
        )
        
        result = await workflow.run([21])
        
        assert result.success
        assert result.result == 42


class TestSagaWorkflow:
    """Tests for SagaWorkflow"""
    
    @pytest.mark.asyncio
    async def test_successful_saga(self):
        """Successful saga completes all steps"""
        saga = SagaWorkflow("test")
        saga.add_saga_step("step1", lambda x: x + 1, lambda x: x - 1)
        saga.add_saga_step("step2", lambda x: x * 2, lambda x: x // 2)
        
        result = await saga.run(10)
        
        assert result.success
        assert result.result == 22  # (10+1)*2
        assert result.steps_completed == 2
    
    @pytest.mark.asyncio
    async def test_saga_compensation_on_failure(self):
        """Saga runs compensations on failure"""
        compensations_run = []
        
        def comp1(x):
            compensations_run.append("comp1")
        
        def comp2(x):
            compensations_run.append("comp2")
        
        def failing_action(x):
            raise ValueError("fail")
        
        saga = SagaWorkflow("test")
        saga.add_saga_step("step1", lambda x: x + 1, comp1)
        saga.add_saga_step("step2", lambda x: x * 2, comp2)
        saga.add_saga_step("step3", failing_action, lambda x: None)
        
        result = await saga.run(10)
        
        assert not result.success
        assert result.steps_completed == 2
        assert compensations_run == ["comp2", "comp1"]  # Reverse order


class TestFanOutWorkflow:
    """Tests for FanOutWorkflow"""
    
    @pytest.mark.asyncio
    async def test_parallel_execution(self):
        """Fan-out executes workers in parallel"""
        results = []
        
        def worker1(x):
            results.append(x * 2)
            return x * 2
        
        def worker2(x):
            results.append(x + 10)
            return x + 10
        
        workflow = FanOutWorkflow(
            name="test",
            workers=[worker1, worker2],
            gather_fn=sum,
        )
        
        result = await workflow.run(5)
        
        assert result.success
        assert result.result == 25  # (5*2) + (5+10) = 10 + 15 = 25
        assert len(results) == 2
    
    @pytest.mark.asyncio
    async def test_partial_failure(self):
        """Fan-out reports partial failures"""
        def good_worker(x):
            return x
        
        def bad_worker(x):
            raise ValueError("fail")
        
        workflow = FanOutWorkflow(
            name="test",
            workers=[good_worker, bad_worker],
            gather_fn=lambda results: results,
        )
        
        result = await workflow.run(5)
        
        assert not result.success
        assert len(result.errors) == 1
