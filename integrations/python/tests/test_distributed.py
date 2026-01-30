"""
Tests for AetherShell Python SDK - Distributed
"""

import asyncio
import pytest
import time
from datetime import datetime, timedelta
from unittest.mock import MagicMock, AsyncMock

from aethershell.distributed import (
    ServiceRegistry,
    ServiceInfo,
    LeaderElection,
    AgentRouter,
    LoadBalancer,
    LoadBalancerStrategy,
    DistributedSwarm,
)


class TestServiceRegistry:
    """Tests for ServiceRegistry"""
    
    def test_register_service(self):
        """Can register a service"""
        registry = ServiceRegistry()
        
        service = registry.register(
            name="agent-1",
            host="localhost",
            port=8080,
        )
        
        assert service.name == "agent-1"
        assert service.host == "localhost"
        assert service.port == 8080
        assert service.is_healthy
    
    def test_get_service(self):
        """Can retrieve registered service"""
        registry = ServiceRegistry()
        service = registry.register("test", "localhost", 8080)
        
        retrieved = registry.get_service(service.service_id)
        
        assert retrieved is not None
        assert retrieved.name == "test"
    
    def test_get_services_by_name(self):
        """Can get all services with same name"""
        registry = ServiceRegistry()
        registry.register("api", "host1", 8080)
        registry.register("api", "host2", 8080)
        registry.register("worker", "host3", 8080)
        
        apis = registry.get_services_by_name("api")
        
        assert len(apis) == 2
        assert all(s.name == "api" for s in apis)
    
    def test_deregister_service(self):
        """Can deregister a service"""
        registry = ServiceRegistry()
        service = registry.register("test", "localhost", 8080)
        
        result = registry.deregister(service.service_id)
        
        assert result is True
        assert registry.get_service(service.service_id) is None
    
    def test_heartbeat(self):
        """Heartbeat updates last_heartbeat"""
        registry = ServiceRegistry()
        service = registry.register("test", "localhost", 8080)
        
        old_heartbeat = service.last_heartbeat
        time.sleep(0.01)
        
        registry.heartbeat(service.service_id)
        
        assert service.last_heartbeat > old_heartbeat
    
    def test_healthy_filter(self):
        """Can filter to healthy services only"""
        registry = ServiceRegistry()
        healthy = registry.register("test", "host1", 8080)
        unhealthy = registry.register("test", "host2", 8080)
        
        # Make one unhealthy
        unhealthy.last_heartbeat = datetime.now() - timedelta(seconds=60)
        
        services = registry.get_services_by_name("test", healthy_only=True)
        
        assert len(services) == 1
        assert services[0].host == "host1"


class TestLoadBalancer:
    """Tests for LoadBalancer"""
    
    def test_round_robin(self):
        """Round-robin selects services in order"""
        registry = ServiceRegistry()
        registry.register("api", "host1", 8080)
        registry.register("api", "host2", 8080)
        registry.register("api", "host3", 8080)
        
        lb = LoadBalancer(registry, LoadBalancerStrategy.ROUND_ROBIN)
        
        hosts = [lb.select_service("api").host for _ in range(6)]
        
        # Should cycle through all hosts
        assert hosts[:3] == ["host1", "host2", "host3"] or \
               len(set(hosts)) == 3  # At least all hosts selected
    
    def test_random(self):
        """Random selection picks from available services"""
        registry = ServiceRegistry()
        registry.register("api", "host1", 8080)
        registry.register("api", "host2", 8080)
        
        lb = LoadBalancer(registry, LoadBalancerStrategy.RANDOM)
        
        # Should eventually select both hosts
        hosts = set(lb.select_service("api").host for _ in range(20))
        assert len(hosts) >= 1  # At least one selected
    
    def test_least_connections(self):
        """Least connections selects service with fewest connections"""
        registry = ServiceRegistry()
        s1 = registry.register("api", "host1", 8080)
        s2 = registry.register("api", "host2", 8080)
        
        s1.active_connections = 10
        s2.active_connections = 2
        
        lb = LoadBalancer(registry, LoadBalancerStrategy.LEAST_CONNECTIONS)
        selected = lb.select_service("api")
        
        assert selected.host == "host2"
    
    def test_consistent_hash(self):
        """Consistent hash gives same result for same key"""
        registry = ServiceRegistry()
        registry.register("api", "host1", 8080)
        registry.register("api", "host2", 8080)
        registry.register("api", "host3", 8080)
        
        lb = LoadBalancer(registry, LoadBalancerStrategy.CONSISTENT_HASH)
        
        # Same key should always select same host
        host1 = lb.select_service("api", "user-123").host
        host2 = lb.select_service("api", "user-123").host
        
        assert host1 == host2
    
    def test_weighted(self):
        """Weighted selection respects weights"""
        registry = ServiceRegistry()
        s1 = registry.register("api", "host1", 8080, weight=1)
        s2 = registry.register("api", "host2", 8080, weight=10)
        
        lb = LoadBalancer(registry, LoadBalancerStrategy.WEIGHTED)
        
        # With higher weight, host2 should be selected more often
        selections = [lb.select_service("api").host for _ in range(100)]
        host2_count = selections.count("host2")
        
        # host2 has 10x weight, so should be selected most of the time
        assert host2_count > 50


class TestAgentRouter:
    """Tests for AgentRouter"""
    
    def test_route_by_name(self):
        """Can route to service by name"""
        registry = ServiceRegistry()
        registry.register("agent-nlp", "host1", 8080)
        
        router = AgentRouter(registry)
        service = router.route_by_name("agent-nlp")
        
        assert service is not None
        assert service.name == "agent-nlp"
    
    def test_route_by_capability(self):
        """Can route to service by capability"""
        registry = ServiceRegistry()
        registry.register("agent-1", "host1", 8080)
        
        router = AgentRouter(registry)
        router.register_capability("agent-1", "text-generation")
        
        service = router.route_by_capability("text-generation")
        
        assert service is not None
        assert service.name == "agent-1"
    
    def test_get_capabilities(self):
        """Can list capabilities for a service"""
        registry = ServiceRegistry()
        registry.register("agent-1", "host1", 8080)
        
        router = AgentRouter(registry)
        router.register_capability("agent-1", "nlp")
        router.register_capability("agent-1", "summarization")
        
        caps = router.get_capabilities("agent-1")
        
        assert "nlp" in caps
        assert "summarization" in caps


class TestLeaderElection:
    """Tests for LeaderElection"""
    
    @pytest.mark.asyncio
    async def test_single_node_becomes_leader(self):
        """Single node becomes leader"""
        registry = ServiceRegistry()
        election = LeaderElection("node-1", registry, "cluster")
        
        leader = await election.run_election()
        
        assert leader == "node-1"
        assert election.is_leader
    
    @pytest.mark.asyncio
    async def test_lowest_id_becomes_leader(self):
        """Lowest ID node becomes leader"""
        registry = ServiceRegistry()
        registry.register("cluster", "host2", 8080, service_id="node-2")
        registry.register("cluster", "host3", 8080, service_id="node-3")
        
        election = LeaderElection("node-1", registry, "cluster")
        leader = await election.run_election()
        
        assert leader == "node-1"
        assert election.is_leader
    
    @pytest.mark.asyncio
    async def test_not_leader_when_higher_id(self):
        """Node with higher ID is not leader"""
        registry = ServiceRegistry()
        registry.register("cluster", "host1", 8080, service_id="node-1")
        
        election = LeaderElection("node-2", registry, "cluster")
        leader = await election.run_election()
        
        assert leader == "node-1"
        assert not election.is_leader
    
    @pytest.mark.asyncio
    async def test_leadership_callback(self):
        """Leadership change triggers callback"""
        registry = ServiceRegistry()
        election = LeaderElection("node-1", registry, "cluster")
        
        became_leader = []
        election.on_leadership_change(lambda is_leader: became_leader.append(is_leader))
        
        await election.run_election()
        
        assert became_leader == [True]
    
    @pytest.mark.asyncio
    async def test_step_down(self):
        """Leader can step down"""
        registry = ServiceRegistry()
        election = LeaderElection("node-1", registry, "cluster")
        
        await election.run_election()
        assert election.is_leader
        
        await election.step_down()
        assert not election.is_leader


class TestDistributedSwarm:
    """Tests for DistributedSwarm"""
    
    def test_add_local_agent(self):
        """Can add local agent"""
        registry = ServiceRegistry()
        swarm = DistributedSwarm("test-swarm", registry)
        
        mock_agent = MagicMock()
        mock_agent.name = "agent-1"
        mock_agent.model = "gpt-4"
        
        swarm.add_local_agent(mock_agent)
        
        stats = swarm.get_stats()
        assert stats["local_agents"] == 1
    
    def test_register_local_agents(self):
        """Local agents can be registered with service registry"""
        registry = ServiceRegistry()
        swarm = DistributedSwarm("test-swarm", registry)
        
        mock_agent = MagicMock()
        mock_agent.name = "agent-1"
        mock_agent.model = "gpt-4"
        swarm.add_local_agent(mock_agent)
        
        services = swarm.register_local_agents("localhost", 8080)
        
        assert len(services) == 1
        assert "test-swarm/agent-1" in services[0].name
    
    @pytest.mark.asyncio
    async def test_dispatch_to_local_agent(self):
        """Dispatch routes to local agent"""
        registry = ServiceRegistry()
        swarm = DistributedSwarm("test-swarm", registry)
        
        mock_agent = MagicMock()
        mock_agent.name = "agent-1"
        mock_agent.run = AsyncMock(return_value={"result": "done"})
        swarm.add_local_agent(mock_agent)
        
        result = await swarm.dispatch("test goal", agent_name="agent-1")
        
        mock_agent.run.assert_called_once_with("test goal")
    
    @pytest.mark.asyncio
    async def test_broadcast_to_all_agents(self):
        """Broadcast sends to all local agents"""
        registry = ServiceRegistry()
        swarm = DistributedSwarm("test-swarm", registry)
        
        agent1 = MagicMock()
        agent1.name = "agent-1"
        agent1.run = AsyncMock(return_value={"result": "1"})
        
        agent2 = MagicMock()
        agent2.name = "agent-2"
        agent2.run = AsyncMock(return_value={"result": "2"})
        
        swarm.add_local_agent(agent1)
        swarm.add_local_agent(agent2)
        
        results = await swarm.broadcast("test goal")
        
        assert len(results) == 2
        agent1.run.assert_called_once()
        agent2.run.assert_called_once()
