"""
AetherShell Distributed Agents for Python

Provides distributed agent coordination:
- Service discovery
- Leader election  
- Agent routing
- Load balancing
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import random
import time
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from enum import Enum
from typing import Any, Callable, Dict, List, Optional, Set
import threading

from . import AetherRuntime, Agent

__all__ = [
    "ServiceRegistry",
    "ServiceInfo",
    "LeaderElection",
    "AgentRouter",
    "LoadBalancer",
    "LoadBalancerStrategy",
    "DistributedSwarm",
]


class LoadBalancerStrategy(Enum):
    """Load balancing strategies"""
    ROUND_ROBIN = "round_robin"
    RANDOM = "random"
    LEAST_CONNECTIONS = "least_connections"
    CONSISTENT_HASH = "consistent_hash"
    WEIGHTED = "weighted"


@dataclass
class ServiceInfo:
    """Information about a registered service"""
    service_id: str
    name: str
    host: str
    port: int
    metadata: Dict[str, Any] = field(default_factory=dict)
    health_check_url: Optional[str] = None
    last_heartbeat: Optional[datetime] = None
    weight: int = 1
    active_connections: int = 0
    
    @property
    def is_healthy(self) -> bool:
        """Check if service is healthy (had heartbeat within 30s)"""
        if self.last_heartbeat is None:
            return False
        return datetime.now() - self.last_heartbeat < timedelta(seconds=30)
    
    @property
    def address(self) -> str:
        """Get service address"""
        return f"{self.host}:{self.port}"
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary"""
        return {
            "service_id": self.service_id,
            "name": self.name,
            "host": self.host,
            "port": self.port,
            "metadata": self.metadata,
            "health_check_url": self.health_check_url,
            "last_heartbeat": self.last_heartbeat.isoformat() if self.last_heartbeat else None,
            "weight": self.weight,
            "active_connections": self.active_connections,
            "is_healthy": self.is_healthy,
        }


class ServiceRegistry:
    """
    Service registry for agent discovery.
    
    Maintains a registry of available agents and their locations.
    """
    
    def __init__(self):
        self._services: Dict[str, ServiceInfo] = {}
        self._by_name: Dict[str, List[str]] = {}
        self._lock = threading.Lock()
        self._watchers: Dict[str, List[Callable[[ServiceInfo], None]]] = {}
    
    def register(
        self,
        name: str,
        host: str,
        port: int,
        service_id: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
        health_check_url: Optional[str] = None,
        weight: int = 1,
    ) -> ServiceInfo:
        """
        Register a service.
        
        Args:
            name: Service name
            host: Service host
            port: Service port
            service_id: Unique service ID (generated if not provided)
            metadata: Additional metadata
            health_check_url: Health check endpoint
            weight: Load balancing weight
            
        Returns:
            ServiceInfo for the registered service
        """
        if service_id is None:
            service_id = f"{name}-{host}-{port}-{int(time.time() * 1000)}"
        
        service = ServiceInfo(
            service_id=service_id,
            name=name,
            host=host,
            port=port,
            metadata=metadata or {},
            health_check_url=health_check_url,
            last_heartbeat=datetime.now(),
            weight=weight,
        )
        
        with self._lock:
            self._services[service_id] = service
            
            if name not in self._by_name:
                self._by_name[name] = []
            if service_id not in self._by_name[name]:
                self._by_name[name].append(service_id)
            
            # Notify watchers
            if name in self._watchers:
                for callback in self._watchers[name]:
                    try:
                        callback(service)
                    except Exception:
                        pass
        
        return service
    
    def deregister(self, service_id: str) -> bool:
        """
        Deregister a service.
        
        Args:
            service_id: Service ID to remove
            
        Returns:
            True if service was removed
        """
        with self._lock:
            if service_id not in self._services:
                return False
            
            service = self._services.pop(service_id)
            
            if service.name in self._by_name:
                self._by_name[service.name] = [
                    sid for sid in self._by_name[service.name]
                    if sid != service_id
                ]
            
            return True
    
    def heartbeat(self, service_id: str) -> bool:
        """
        Update service heartbeat.
        
        Args:
            service_id: Service ID
            
        Returns:
            True if service exists
        """
        with self._lock:
            if service_id in self._services:
                self._services[service_id].last_heartbeat = datetime.now()
                return True
            return False
    
    def get_service(self, service_id: str) -> Optional[ServiceInfo]:
        """Get service by ID"""
        return self._services.get(service_id)
    
    def get_services_by_name(self, name: str, healthy_only: bool = True) -> List[ServiceInfo]:
        """
        Get all services with a given name.
        
        Args:
            name: Service name
            healthy_only: Only return healthy services
            
        Returns:
            List of matching services
        """
        with self._lock:
            if name not in self._by_name:
                return []
            
            services = [
                self._services[sid]
                for sid in self._by_name[name]
                if sid in self._services
            ]
            
            if healthy_only:
                services = [s for s in services if s.is_healthy]
            
            return services
    
    def get_all_services(self, healthy_only: bool = True) -> List[ServiceInfo]:
        """Get all registered services"""
        with self._lock:
            services = list(self._services.values())
            if healthy_only:
                services = [s for s in services if s.is_healthy]
            return services
    
    def watch(self, name: str, callback: Callable[[ServiceInfo], None]) -> None:
        """
        Watch for changes to services with a given name.
        
        Args:
            name: Service name to watch
            callback: Function called when service changes
        """
        with self._lock:
            if name not in self._watchers:
                self._watchers[name] = []
            self._watchers[name].append(callback)
    
    def unwatch(self, name: str, callback: Callable[[ServiceInfo], None]) -> None:
        """Remove a watcher"""
        with self._lock:
            if name in self._watchers:
                self._watchers[name] = [
                    cb for cb in self._watchers[name]
                    if cb != callback
                ]
    
    async def cleanup_stale_services(self, max_age_seconds: int = 60) -> int:
        """
        Remove services that haven't sent heartbeats.
        
        Args:
            max_age_seconds: Maximum age without heartbeat
            
        Returns:
            Number of services removed
        """
        cutoff = datetime.now() - timedelta(seconds=max_age_seconds)
        removed = 0
        
        with self._lock:
            stale_ids = [
                sid for sid, service in self._services.items()
                if service.last_heartbeat and service.last_heartbeat < cutoff
            ]
        
        for sid in stale_ids:
            if self.deregister(sid):
                removed += 1
        
        return removed


class LeaderElection:
    """
    Simple leader election for distributed coordination.
    
    Uses a basic election algorithm where the lowest ID wins.
    """
    
    def __init__(
        self,
        node_id: str,
        registry: ServiceRegistry,
        election_group: str = "default",
    ):
        self.node_id = node_id
        self.registry = registry
        self.election_group = election_group
        self._is_leader = False
        self._current_leader: Optional[str] = None
        self._lock = threading.Lock()
        self._callbacks: List[Callable[[bool], None]] = []
    
    @property
    def is_leader(self) -> bool:
        """Check if this node is the leader"""
        return self._is_leader
    
    @property
    def current_leader(self) -> Optional[str]:
        """Get current leader ID"""
        return self._current_leader
    
    def on_leadership_change(self, callback: Callable[[bool], None]) -> None:
        """
        Register callback for leadership changes.
        
        Args:
            callback: Function called with True when becoming leader
        """
        self._callbacks.append(callback)
    
    async def run_election(self) -> str:
        """
        Run leader election.
        
        Returns:
            ID of the elected leader
        """
        # Get all nodes in election group
        services = self.registry.get_services_by_name(self.election_group)
        
        if not services:
            # No other nodes, we're the leader
            with self._lock:
                was_leader = self._is_leader
                self._is_leader = True
                self._current_leader = self.node_id
            
            if not was_leader:
                for callback in self._callbacks:
                    try:
                        callback(True)
                    except Exception:
                        pass
            
            return self.node_id
        
        # Find lowest ID (simple election)
        all_ids = [s.service_id for s in services]
        all_ids.append(self.node_id)
        leader_id = min(all_ids)
        
        with self._lock:
            was_leader = self._is_leader
            self._is_leader = leader_id == self.node_id
            self._current_leader = leader_id
        
        if was_leader != self._is_leader:
            for callback in self._callbacks:
                try:
                    callback(self._is_leader)
                except Exception:
                    pass
        
        return leader_id
    
    async def step_down(self) -> None:
        """Voluntarily step down as leader"""
        with self._lock:
            was_leader = self._is_leader
            self._is_leader = False
        
        if was_leader:
            for callback in self._callbacks:
                try:
                    callback(False)
                except Exception:
                    pass


class LoadBalancer:
    """
    Load balancer for distributing requests across services.
    """
    
    def __init__(
        self,
        registry: ServiceRegistry,
        strategy: LoadBalancerStrategy = LoadBalancerStrategy.ROUND_ROBIN,
    ):
        self.registry = registry
        self.strategy = strategy
        self._round_robin_index: Dict[str, int] = {}
        self._lock = threading.Lock()
    
    def select_service(self, name: str, key: Optional[str] = None) -> Optional[ServiceInfo]:
        """
        Select a service instance using the configured strategy.
        
        Args:
            name: Service name
            key: Optional key for consistent hashing
            
        Returns:
            Selected service or None
        """
        services = self.registry.get_services_by_name(name)
        
        if not services:
            return None
        
        if len(services) == 1:
            return services[0]
        
        if self.strategy == LoadBalancerStrategy.ROUND_ROBIN:
            return self._select_round_robin(name, services)
        elif self.strategy == LoadBalancerStrategy.RANDOM:
            return self._select_random(services)
        elif self.strategy == LoadBalancerStrategy.LEAST_CONNECTIONS:
            return self._select_least_connections(services)
        elif self.strategy == LoadBalancerStrategy.CONSISTENT_HASH:
            return self._select_consistent_hash(services, key or name)
        elif self.strategy == LoadBalancerStrategy.WEIGHTED:
            return self._select_weighted(services)
        
        return services[0]
    
    def _select_round_robin(self, name: str, services: List[ServiceInfo]) -> ServiceInfo:
        """Round-robin selection"""
        with self._lock:
            idx = self._round_robin_index.get(name, 0)
            self._round_robin_index[name] = (idx + 1) % len(services)
        return services[idx % len(services)]
    
    def _select_random(self, services: List[ServiceInfo]) -> ServiceInfo:
        """Random selection"""
        return random.choice(services)
    
    def _select_least_connections(self, services: List[ServiceInfo]) -> ServiceInfo:
        """Select service with fewest active connections"""
        return min(services, key=lambda s: s.active_connections)
    
    def _select_consistent_hash(self, services: List[ServiceInfo], key: str) -> ServiceInfo:
        """Consistent hash selection"""
        hash_val = int(hashlib.md5(key.encode()).hexdigest(), 16)
        idx = hash_val % len(services)
        return services[idx]
    
    def _select_weighted(self, services: List[ServiceInfo]) -> ServiceInfo:
        """Weighted random selection"""
        total_weight = sum(s.weight for s in services)
        target = random.randint(1, total_weight)
        
        cumulative = 0
        for service in services:
            cumulative += service.weight
            if cumulative >= target:
                return service
        
        return services[-1]


class AgentRouter:
    """
    Routes agent requests to appropriate handlers.
    
    Supports capability-based routing and load balancing.
    """
    
    def __init__(
        self,
        registry: ServiceRegistry,
        load_balancer: Optional[LoadBalancer] = None,
    ):
        self.registry = registry
        self.load_balancer = load_balancer or LoadBalancer(registry)
        self._capability_map: Dict[str, Set[str]] = {}
        self._lock = threading.Lock()
    
    def register_capability(self, service_name: str, capability: str) -> None:
        """
        Register a capability for a service.
        
        Args:
            service_name: Name of the service
            capability: Capability identifier
        """
        with self._lock:
            if capability not in self._capability_map:
                self._capability_map[capability] = set()
            self._capability_map[capability].add(service_name)
    
    def route_by_capability(self, capability: str) -> Optional[ServiceInfo]:
        """
        Find a service that provides a capability.
        
        Args:
            capability: Required capability
            
        Returns:
            Service that provides the capability
        """
        with self._lock:
            service_names = self._capability_map.get(capability, set())
        
        for name in service_names:
            service = self.load_balancer.select_service(name)
            if service:
                return service
        
        return None
    
    def route_by_name(self, name: str, key: Optional[str] = None) -> Optional[ServiceInfo]:
        """
        Route to a service by name.
        
        Args:
            name: Service name
            key: Optional routing key for consistent hashing
            
        Returns:
            Selected service instance
        """
        return self.load_balancer.select_service(name, key)
    
    def get_capabilities(self, service_name: str) -> Set[str]:
        """Get all capabilities for a service"""
        with self._lock:
            return {
                cap for cap, services in self._capability_map.items()
                if service_name in services
            }


class DistributedSwarm:
    """
    Distributed swarm of agents across multiple nodes.
    
    Coordinates agent execution across a cluster.
    """
    
    def __init__(
        self,
        name: str,
        registry: ServiceRegistry,
        runtime: Optional[AetherRuntime] = None,
    ):
        self.name = name
        self.registry = registry
        self._runtime = runtime or AetherRuntime()
        self.router = AgentRouter(registry)
        self._local_agents: Dict[str, Agent] = {}
        self._lock = threading.Lock()
    
    def add_local_agent(self, agent: Agent) -> None:
        """
        Add a locally-running agent.
        
        Args:
            agent: Agent instance
        """
        with self._lock:
            self._local_agents[agent.name] = agent
    
    def register_local_agents(
        self,
        host: str,
        port: int,
        capabilities: Optional[List[str]] = None,
    ) -> List[ServiceInfo]:
        """
        Register local agents with the service registry.
        
        Args:
            host: Host address
            port: Service port
            capabilities: Agent capabilities
            
        Returns:
            List of registered services
        """
        services = []
        
        with self._lock:
            for agent_name, agent in self._local_agents.items():
                service = self.registry.register(
                    name=f"{self.name}/{agent_name}",
                    host=host,
                    port=port,
                    metadata={"agent_name": agent_name, "model": agent.model},
                )
                services.append(service)
                
                # Register capabilities
                for cap in (capabilities or []):
                    self.router.register_capability(service.name, cap)
        
        return services
    
    async def dispatch(
        self,
        goal: str,
        agent_name: Optional[str] = None,
        capability: Optional[str] = None,
        prefer_local: bool = True,
    ) -> Dict[str, Any]:
        """
        Dispatch a task to an agent.
        
        Args:
            goal: Task goal
            agent_name: Specific agent to use
            capability: Required capability
            prefer_local: Prefer local agents
            
        Returns:
            Agent response
        """
        # Try local agent first if preferred
        if prefer_local and agent_name:
            with self._lock:
                if agent_name in self._local_agents:
                    return await self._local_agents[agent_name].run(goal)
        
        # Find remote service
        service: Optional[ServiceInfo] = None
        
        if capability:
            service = self.router.route_by_capability(capability)
        elif agent_name:
            service = self.router.route_by_name(f"{self.name}/{agent_name}")
        
        if service:
            # Would call remote service here
            # For now, return a placeholder
            return {
                "status": "routed",
                "service": service.to_dict(),
                "goal": goal,
            }
        
        # Fallback to any local agent
        with self._lock:
            if self._local_agents:
                agent = next(iter(self._local_agents.values()))
                return await agent.run(goal)
        
        return {"error": "No available agents"}
    
    async def broadcast(self, goal: str) -> List[Dict[str, Any]]:
        """
        Broadcast a task to all agents.
        
        Args:
            goal: Task goal
            
        Returns:
            List of responses from all agents
        """
        tasks = []
        
        # Local agents
        with self._lock:
            for agent in self._local_agents.values():
                tasks.append(asyncio.create_task(agent.run(goal)))
        
        # Would also dispatch to remote agents here
        
        if tasks:
            results = await asyncio.gather(*tasks, return_exceptions=True)
            return [
                r if not isinstance(r, Exception) else {"error": str(r)}
                for r in results
            ]
        
        return []
    
    def get_stats(self) -> Dict[str, Any]:
        """Get swarm statistics"""
        with self._lock:
            local_count = len(self._local_agents)
        
        remote_services = self.registry.get_services_by_name(self.name)
        
        return {
            "name": self.name,
            "local_agents": local_count,
            "remote_services": len(remote_services),
            "total_agents": local_count + len(remote_services),
        }
