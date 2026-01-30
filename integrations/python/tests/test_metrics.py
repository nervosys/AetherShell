"""
Tests for AetherShell Python SDK - Metrics
"""

import asyncio
import pytest
import time
from unittest.mock import MagicMock

from aethershell.metrics import (
    Counter,
    Gauge,
    Histogram,
    Timer,
    HealthCheck,
    HealthStatus,
    Span,
    Tracer,
    MetricsCollector,
    get_metrics_collector,
    metrics_middleware,
)


class TestCounter:
    """Tests for Counter metric"""
    
    def test_initial_value(self):
        """Counter starts at zero"""
        counter = Counter(name="test_counter")
        assert counter.value == 0
    
    def test_increment(self):
        """Counter increments correctly"""
        counter = Counter(name="test_counter")
        counter.inc()
        assert counter.value == 1
        counter.inc(5)
        assert counter.value == 6
    
    def test_prometheus_format(self):
        """Counter exports to Prometheus format"""
        counter = Counter(name="requests_total", labels={"method": "GET"})
        counter.inc(42)
        output = counter.to_prometheus()
        assert 'requests_total{method="GET"}' in output
        assert "42" in output


class TestGauge:
    """Tests for Gauge metric"""
    
    def test_set_value(self):
        """Gauge can be set to any value"""
        gauge = Gauge(name="test_gauge")
        gauge.set(42)
        assert gauge.value == 42
        gauge.set(-10)
        assert gauge.value == -10
    
    def test_increment_decrement(self):
        """Gauge can increment and decrement"""
        gauge = Gauge(name="test_gauge")
        gauge.set(10)
        gauge.inc(5)
        assert gauge.value == 15
        gauge.dec(3)
        assert gauge.value == 12
    
    def test_prometheus_format(self):
        """Gauge exports to Prometheus format"""
        gauge = Gauge(name="connections_active")
        gauge.set(100)
        output = gauge.to_prometheus()
        assert "connections_active" in output
        assert "100" in output


class TestHistogram:
    """Tests for Histogram metric"""
    
    def test_observe(self):
        """Histogram records observations"""
        hist = Histogram(name="request_duration")
        hist.observe(0.1)
        hist.observe(0.2)
        hist.observe(0.3)
        
        assert hist.count == 3
        assert abs(hist.sum - 0.6) < 0.001
    
    def test_mean(self):
        """Histogram calculates mean correctly"""
        hist = Histogram(name="test")
        hist.observe(10)
        hist.observe(20)
        hist.observe(30)
        
        assert hist.mean == 20
    
    def test_percentile(self):
        """Histogram calculates percentiles"""
        hist = Histogram(name="test")
        for i in range(100):
            hist.observe(i)
        
        p50 = hist.percentile(50)
        assert 45 <= p50 <= 55  # Approximately 50
    
    def test_prometheus_format(self):
        """Histogram exports to Prometheus format"""
        hist = Histogram(name="latency", buckets=[0.1, 0.5, 1.0])
        hist.observe(0.2)
        hist.observe(0.7)
        
        output = hist.to_prometheus()
        assert "latency_bucket" in output
        assert "latency_sum" in output
        assert "latency_count" in output


class TestTimer:
    """Tests for Timer context manager"""
    
    def test_timer_records_duration(self):
        """Timer records duration to histogram"""
        hist = Histogram(name="test")
        
        with Timer(hist):
            time.sleep(0.01)  # 10ms
        
        assert hist.count == 1
        assert hist.sum >= 0.01


class TestTracer:
    """Tests for Tracer"""
    
    def test_create_span(self):
        """Tracer creates spans"""
        tracer = Tracer(service_name="test-service")
        
        with tracer.start_span("test-span") as span:
            span.set_attribute("key", "value")
        
        spans = tracer.get_spans()
        assert len(spans) == 1
        assert spans[0].name == "test-span"
        assert spans[0].attributes["key"] == "value"
    
    def test_nested_spans(self):
        """Tracer handles nested spans"""
        tracer = Tracer(service_name="test-service")
        
        with tracer.start_span("parent") as parent:
            with tracer.start_span("child", parent=parent) as child:
                pass
        
        spans = tracer.get_spans()
        assert len(spans) == 2
        
        parent_span = [s for s in spans if s.name == "parent"][0]
        child_span = [s for s in spans if s.name == "child"][0]
        
        assert child_span.parent_id == parent_span.span_id
        assert child_span.trace_id == parent_span.trace_id
    
    def test_span_error_status(self):
        """Span captures error status"""
        tracer = Tracer(service_name="test-service")
        
        try:
            with tracer.start_span("failing-span") as span:
                raise ValueError("test error")
        except ValueError:
            pass
        
        spans = tracer.get_spans()
        assert spans[0].status == "ERROR"
    
    def test_span_duration(self):
        """Span records duration"""
        tracer = Tracer(service_name="test-service")
        
        with tracer.start_span("timed-span") as span:
            time.sleep(0.01)
        
        spans = tracer.get_spans()
        assert spans[0].duration_ms >= 10


class TestHealthCheck:
    """Tests for HealthCheck"""
    
    @pytest.mark.asyncio
    async def test_healthy_check(self):
        """Healthy check returns HEALTHY"""
        check = HealthCheck(name="test", check_fn=lambda: True)
        status = await check.check()
        assert status == HealthStatus.HEALTHY
    
    @pytest.mark.asyncio
    async def test_unhealthy_check(self):
        """Failed check returns UNHEALTHY after threshold"""
        check = HealthCheck(name="test", check_fn=lambda: False)
        
        # First few failures are DEGRADED
        for _ in range(2):
            await check.check()
        
        # After threshold, becomes UNHEALTHY
        status = await check.check()
        assert status == HealthStatus.UNHEALTHY


class TestMetricsCollector:
    """Tests for MetricsCollector"""
    
    def test_create_counter(self):
        """Collector creates counters"""
        collector = MetricsCollector(namespace="test")
        counter = collector.counter("requests", "Total requests")
        
        counter.inc()
        
        assert counter.name == "test_requests"
        assert counter.value == 1
    
    def test_create_gauge(self):
        """Collector creates gauges"""
        collector = MetricsCollector(namespace="test")
        gauge = collector.gauge("connections", "Active connections")
        
        gauge.set(42)
        
        assert gauge.name == "test_connections"
        assert gauge.value == 42
    
    def test_create_histogram(self):
        """Collector creates histograms"""
        collector = MetricsCollector(namespace="test")
        hist = collector.histogram("latency", "Request latency")
        
        hist.observe(0.1)
        
        assert hist.name == "test_latency"
        assert hist.count == 1
    
    def test_prometheus_export(self):
        """Collector exports all metrics to Prometheus format"""
        collector = MetricsCollector(namespace="app")
        
        counter = collector.counter("requests")
        counter.inc(100)
        
        gauge = collector.gauge("memory")
        gauge.set(1024)
        
        output = collector.to_prometheus()
        
        assert "app_requests" in output
        assert "app_memory" in output
    
    def test_json_export(self):
        """Collector exports metrics as JSON"""
        collector = MetricsCollector(namespace="app")
        
        counter = collector.counter("events")
        counter.inc(50)
        
        json_output = collector.to_json()
        
        assert "app_events" in json_output
        assert "50" in json_output


class TestMetricsMiddleware:
    """Tests for metrics_middleware decorator"""
    
    def test_tracks_calls(self):
        """Middleware tracks function calls"""
        collector = MetricsCollector(namespace="test")
        collector.reset()
        
        @metrics_middleware(collector)
        def my_function():
            return 42
        
        my_function()
        my_function()
        
        json_data = collector.to_json()
        assert "my_function_calls_total" in json_data
    
    def test_tracks_errors(self):
        """Middleware tracks errors"""
        collector = MetricsCollector(namespace="test")
        collector.reset()
        
        @metrics_middleware(collector)
        def failing_function():
            raise ValueError("fail")
        
        with pytest.raises(ValueError):
            failing_function()
        
        json_data = collector.to_json()
        assert "failing_function_errors_total" in json_data
