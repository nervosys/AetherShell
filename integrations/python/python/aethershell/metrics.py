"""
AetherShell Metrics and Observability for Python

Provides metrics collection, tracing, and monitoring:
- Prometheus-compatible metrics
- OpenTelemetry integration
- Health checks
- Performance tracking
"""

from __future__ import annotations

import asyncio
import json
import time
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from enum import Enum
from typing import Any, Callable, Dict, List, Optional, TypeVar
from contextlib import contextmanager
import threading

__all__ = [
    "MetricsCollector",
    "Counter",
    "Gauge",
    "Histogram",
    "Timer",
    "HealthCheck",
    "HealthStatus",
    "Span",
    "Tracer",
    "metrics_middleware",
]

T = TypeVar("T")


class HealthStatus(Enum):
    """Health check status"""
    HEALTHY = "healthy"
    DEGRADED = "degraded"
    UNHEALTHY = "unhealthy"


@dataclass
class Counter:
    """
    Counter metric - monotonically increasing value.
    
    Use for: request counts, error counts, completed tasks.
    """
    name: str
    description: str = ""
    labels: Dict[str, str] = field(default_factory=dict)
    _value: float = 0.0
    _lock: threading.Lock = field(default_factory=threading.Lock)
    
    def inc(self, amount: float = 1.0) -> None:
        """Increment the counter"""
        with self._lock:
            self._value += amount
    
    @property
    def value(self) -> float:
        """Get current value"""
        return self._value
    
    def to_prometheus(self) -> str:
        """Export in Prometheus format"""
        labels_str = ",".join(f'{k}="{v}"' for k, v in self.labels.items())
        label_part = f"{{{labels_str}}}" if labels_str else ""
        return f"{self.name}{label_part} {self._value}"


@dataclass
class Gauge:
    """
    Gauge metric - value that can go up and down.
    
    Use for: current connections, queue size, memory usage.
    """
    name: str
    description: str = ""
    labels: Dict[str, str] = field(default_factory=dict)
    _value: float = 0.0
    _lock: threading.Lock = field(default_factory=threading.Lock)
    
    def set(self, value: float) -> None:
        """Set the gauge value"""
        with self._lock:
            self._value = value
    
    def inc(self, amount: float = 1.0) -> None:
        """Increment the gauge"""
        with self._lock:
            self._value += amount
    
    def dec(self, amount: float = 1.0) -> None:
        """Decrement the gauge"""
        with self._lock:
            self._value -= amount
    
    @property
    def value(self) -> float:
        """Get current value"""
        return self._value
    
    def to_prometheus(self) -> str:
        """Export in Prometheus format"""
        labels_str = ",".join(f'{k}="{v}"' for k, v in self.labels.items())
        label_part = f"{{{labels_str}}}" if labels_str else ""
        return f"{self.name}{label_part} {self._value}"


@dataclass
class Histogram:
    """
    Histogram metric - distribution of values.
    
    Use for: request latency, response sizes.
    """
    name: str
    description: str = ""
    buckets: List[float] = field(default_factory=lambda: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0])
    labels: Dict[str, str] = field(default_factory=dict)
    _values: List[float] = field(default_factory=list)
    _bucket_counts: Dict[float, int] = field(default_factory=dict)
    _sum: float = 0.0
    _count: int = 0
    _lock: threading.Lock = field(default_factory=threading.Lock)
    
    def __post_init__(self):
        # Initialize bucket counts
        for bucket in self.buckets:
            self._bucket_counts[bucket] = 0
        self._bucket_counts[float('inf')] = 0
    
    def observe(self, value: float) -> None:
        """Record an observation"""
        with self._lock:
            self._values.append(value)
            self._sum += value
            self._count += 1
            
            # Update bucket counts
            for bucket in self.buckets:
                if value <= bucket:
                    self._bucket_counts[bucket] += 1
            self._bucket_counts[float('inf')] += 1
    
    @property
    def count(self) -> int:
        """Get observation count"""
        return self._count
    
    @property
    def sum(self) -> float:
        """Get sum of observations"""
        return self._sum
    
    @property
    def mean(self) -> float:
        """Get mean value"""
        return self._sum / self._count if self._count > 0 else 0.0
    
    def percentile(self, p: float) -> float:
        """Get percentile value"""
        if not self._values:
            return 0.0
        sorted_values = sorted(self._values)
        idx = int(len(sorted_values) * p / 100)
        return sorted_values[min(idx, len(sorted_values) - 1)]
    
    def to_prometheus(self) -> str:
        """Export in Prometheus format"""
        lines = []
        labels_str = ",".join(f'{k}="{v}"' for k, v in self.labels.items())
        base_labels = f",{labels_str}" if labels_str else ""
        
        for bucket, count in sorted(self._bucket_counts.items()):
            le = "+Inf" if bucket == float('inf') else str(bucket)
            lines.append(f'{self.name}_bucket{{le="{le}"{base_labels}}} {count}')
        
        lines.append(f"{self.name}_sum{{{labels_str}}} {self._sum}" if labels_str else f"{self.name}_sum {self._sum}")
        lines.append(f"{self.name}_count{{{labels_str}}} {self._count}" if labels_str else f"{self.name}_count {self._count}")
        
        return "\n".join(lines)


class Timer:
    """
    Context manager for timing code blocks.
    
    Records duration to a histogram metric.
    """
    
    def __init__(self, histogram: Histogram):
        self.histogram = histogram
        self._start: Optional[float] = None
    
    def __enter__(self) -> Timer:
        self._start = time.perf_counter()
        return self
    
    def __exit__(self, *args) -> None:
        if self._start is not None:
            duration = time.perf_counter() - self._start
            self.histogram.observe(duration)


@dataclass
class Span:
    """
    Tracing span for distributed tracing.
    """
    name: str
    trace_id: str
    span_id: str
    parent_id: Optional[str] = None
    start_time: Optional[datetime] = None
    end_time: Optional[datetime] = None
    attributes: Dict[str, Any] = field(default_factory=dict)
    events: List[Dict[str, Any]] = field(default_factory=list)
    status: str = "OK"
    
    def set_attribute(self, key: str, value: Any) -> None:
        """Set span attribute"""
        self.attributes[key] = value
    
    def add_event(self, name: str, attributes: Optional[Dict[str, Any]] = None) -> None:
        """Add event to span"""
        self.events.append({
            "name": name,
            "timestamp": datetime.now().isoformat(),
            "attributes": attributes or {},
        })
    
    def set_status(self, status: str, description: Optional[str] = None) -> None:
        """Set span status"""
        self.status = status
        if description:
            self.attributes["status_description"] = description
    
    @property
    def duration_ms(self) -> float:
        """Get span duration in milliseconds"""
        if self.start_time and self.end_time:
            return (self.end_time - self.start_time).total_seconds() * 1000
        return 0.0
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary"""
        return {
            "name": self.name,
            "traceId": self.trace_id,
            "spanId": self.span_id,
            "parentId": self.parent_id,
            "startTime": self.start_time.isoformat() if self.start_time else None,
            "endTime": self.end_time.isoformat() if self.end_time else None,
            "attributes": self.attributes,
            "events": self.events,
            "status": self.status,
            "durationMs": self.duration_ms,
        }


class Tracer:
    """
    Simple tracer for distributed tracing.
    
    Creates and manages spans for request tracing.
    """
    
    def __init__(self, service_name: str):
        self.service_name = service_name
        self._spans: List[Span] = []
        self._current_span: Optional[Span] = None
        self._lock = threading.Lock()
    
    def _generate_id(self) -> str:
        """Generate unique ID"""
        import random
        return f"{random.randint(0, 0xFFFFFFFFFFFFFFFF):016x}"
    
    @contextmanager
    def start_span(
        self,
        name: str,
        parent: Optional[Span] = None,
        attributes: Optional[Dict[str, Any]] = None,
    ):
        """
        Start a new span.
        
        Args:
            name: Span name
            parent: Parent span (optional)
            attributes: Initial attributes
            
        Yields:
            Span instance
        """
        trace_id = parent.trace_id if parent else self._generate_id()
        span = Span(
            name=name,
            trace_id=trace_id,
            span_id=self._generate_id(),
            parent_id=parent.span_id if parent else None,
            start_time=datetime.now(),
            attributes={"service.name": self.service_name, **(attributes or {})},
        )
        
        prev_span = self._current_span
        self._current_span = span
        
        try:
            yield span
            span.set_status("OK")
        except Exception as e:
            span.set_status("ERROR", str(e))
            raise
        finally:
            span.end_time = datetime.now()
            with self._lock:
                self._spans.append(span)
            self._current_span = prev_span
    
    @property
    def current_span(self) -> Optional[Span]:
        """Get current active span"""
        return self._current_span
    
    def get_spans(self) -> List[Span]:
        """Get all recorded spans"""
        return self._spans.copy()
    
    def clear_spans(self) -> None:
        """Clear recorded spans"""
        with self._lock:
            self._spans.clear()
    
    def export_json(self) -> str:
        """Export spans as JSON"""
        return json.dumps([s.to_dict() for s in self._spans], indent=2)


@dataclass
class HealthCheck:
    """
    Health check for service monitoring.
    """
    name: str
    check_fn: Callable[[], bool]
    timeout_ms: int = 5000
    interval_ms: int = 30000
    _last_check: Optional[datetime] = None
    _last_status: HealthStatus = HealthStatus.HEALTHY
    _consecutive_failures: int = 0
    
    async def check(self) -> HealthStatus:
        """Run health check"""
        try:
            # Run check with timeout
            if asyncio.iscoroutinefunction(self.check_fn):
                result = await asyncio.wait_for(
                    self.check_fn(),
                    timeout=self.timeout_ms / 1000,
                )
            else:
                result = await asyncio.wait_for(
                    asyncio.to_thread(self.check_fn),
                    timeout=self.timeout_ms / 1000,
                )
            
            if result:
                self._consecutive_failures = 0
                self._last_status = HealthStatus.HEALTHY
            else:
                self._consecutive_failures += 1
                self._last_status = HealthStatus.DEGRADED if self._consecutive_failures < 3 else HealthStatus.UNHEALTHY
        except asyncio.TimeoutError:
            self._consecutive_failures += 1
            self._last_status = HealthStatus.UNHEALTHY
        except Exception:
            self._consecutive_failures += 1
            self._last_status = HealthStatus.UNHEALTHY
        
        self._last_check = datetime.now()
        return self._last_status
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary"""
        return {
            "name": self.name,
            "status": self._last_status.value,
            "lastCheck": self._last_check.isoformat() if self._last_check else None,
            "consecutiveFailures": self._consecutive_failures,
        }


class MetricsCollector:
    """
    Central metrics collector.
    
    Aggregates all metrics and provides export functionality.
    """
    
    def __init__(self, namespace: str = "aethershell"):
        self.namespace = namespace
        self._counters: Dict[str, Counter] = {}
        self._gauges: Dict[str, Gauge] = {}
        self._histograms: Dict[str, Histogram] = {}
        self._health_checks: Dict[str, HealthCheck] = {}
        self._tracer: Optional[Tracer] = None
        self._lock = threading.Lock()
    
    def counter(
        self,
        name: str,
        description: str = "",
        labels: Optional[Dict[str, str]] = None,
    ) -> Counter:
        """Get or create a counter"""
        full_name = f"{self.namespace}_{name}"
        with self._lock:
            if full_name not in self._counters:
                self._counters[full_name] = Counter(
                    name=full_name,
                    description=description,
                    labels=labels or {},
                )
            return self._counters[full_name]
    
    def gauge(
        self,
        name: str,
        description: str = "",
        labels: Optional[Dict[str, str]] = None,
    ) -> Gauge:
        """Get or create a gauge"""
        full_name = f"{self.namespace}_{name}"
        with self._lock:
            if full_name not in self._gauges:
                self._gauges[full_name] = Gauge(
                    name=full_name,
                    description=description,
                    labels=labels or {},
                )
            return self._gauges[full_name]
    
    def histogram(
        self,
        name: str,
        description: str = "",
        buckets: Optional[List[float]] = None,
        labels: Optional[Dict[str, str]] = None,
    ) -> Histogram:
        """Get or create a histogram"""
        full_name = f"{self.namespace}_{name}"
        with self._lock:
            if full_name not in self._histograms:
                self._histograms[full_name] = Histogram(
                    name=full_name,
                    description=description,
                    buckets=buckets or [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
                    labels=labels or {},
                )
            return self._histograms[full_name]
    
    def register_health_check(
        self,
        name: str,
        check_fn: Callable[[], bool],
        timeout_ms: int = 5000,
    ) -> HealthCheck:
        """Register a health check"""
        with self._lock:
            self._health_checks[name] = HealthCheck(
                name=name,
                check_fn=check_fn,
                timeout_ms=timeout_ms,
            )
            return self._health_checks[name]
    
    def tracer(self, service_name: Optional[str] = None) -> Tracer:
        """Get or create tracer"""
        if self._tracer is None:
            self._tracer = Tracer(service_name or self.namespace)
        return self._tracer
    
    async def run_health_checks(self) -> Dict[str, HealthStatus]:
        """Run all health checks"""
        results = {}
        for name, check in self._health_checks.items():
            results[name] = await check.check()
        return results
    
    def to_prometheus(self) -> str:
        """Export all metrics in Prometheus format"""
        lines = []
        
        for counter in self._counters.values():
            if counter.description:
                lines.append(f"# HELP {counter.name} {counter.description}")
            lines.append(f"# TYPE {counter.name} counter")
            lines.append(counter.to_prometheus())
        
        for gauge in self._gauges.values():
            if gauge.description:
                lines.append(f"# HELP {gauge.name} {gauge.description}")
            lines.append(f"# TYPE {gauge.name} gauge")
            lines.append(gauge.to_prometheus())
        
        for histogram in self._histograms.values():
            if histogram.description:
                lines.append(f"# HELP {histogram.name} {histogram.description}")
            lines.append(f"# TYPE {histogram.name} histogram")
            lines.append(histogram.to_prometheus())
        
        return "\n".join(lines)
    
    def to_json(self) -> str:
        """Export all metrics as JSON"""
        data = {
            "counters": {k: v.value for k, v in self._counters.items()},
            "gauges": {k: v.value for k, v in self._gauges.items()},
            "histograms": {
                k: {
                    "count": v.count,
                    "sum": v.sum,
                    "mean": v.mean,
                    "p50": v.percentile(50),
                    "p95": v.percentile(95),
                    "p99": v.percentile(99),
                }
                for k, v in self._histograms.items()
            },
            "health_checks": {k: v.to_dict() for k, v in self._health_checks.items()},
        }
        return json.dumps(data, indent=2)
    
    def reset(self) -> None:
        """Reset all metrics"""
        with self._lock:
            self._counters.clear()
            self._gauges.clear()
            self._histograms.clear()


# Singleton metrics collector
_default_collector: Optional[MetricsCollector] = None


def get_metrics_collector() -> MetricsCollector:
    """Get the default metrics collector"""
    global _default_collector
    if _default_collector is None:
        _default_collector = MetricsCollector()
    return _default_collector


def metrics_middleware(collector: Optional[MetricsCollector] = None):
    """
    Decorator for adding metrics to functions.
    
    Tracks call count, errors, and latency.
    """
    mc = collector or get_metrics_collector()
    
    def decorator(func: Callable[..., T]) -> Callable[..., T]:
        name = func.__name__
        calls = mc.counter(f"{name}_calls_total", "Total function calls")
        errors = mc.counter(f"{name}_errors_total", "Total function errors")
        latency = mc.histogram(f"{name}_latency_seconds", "Function latency")
        
        if asyncio.iscoroutinefunction(func):
            async def async_wrapper(*args, **kwargs) -> T:
                calls.inc()
                start = time.perf_counter()
                try:
                    return await func(*args, **kwargs)
                except Exception as e:
                    errors.inc()
                    raise
                finally:
                    latency.observe(time.perf_counter() - start)
            return async_wrapper
        else:
            def sync_wrapper(*args, **kwargs) -> T:
                calls.inc()
                start = time.perf_counter()
                try:
                    return func(*args, **kwargs)
                except Exception as e:
                    errors.inc()
                    raise
                finally:
                    latency.observe(time.perf_counter() - start)
            return sync_wrapper
    
    return decorator
