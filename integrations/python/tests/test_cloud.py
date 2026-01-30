"""
Tests for AetherShell Python SDK - Cloud Platform
"""

import pytest
import json
import os
import tempfile
from unittest.mock import MagicMock, patch

from aethershell.cloud import (
    CloudProvider,
    FunctionConfig,
    DeploymentConfig,
    LambdaRuntime,
    AzureFunctionsRuntime,
    GCPFunctionsRuntime,
    KnativeRuntime,
    get_runtime,
    create_handler,
    deploy_agent,
)


SAMPLE_AGENT_CODE = '''
def create_agent(runtime):
    """Create the agent for this function"""
    from aethershell import Agent
    return Agent(
        name="test-agent",
        model="openai:gpt-4o-mini",
        runtime=runtime,
    )
'''


class TestFunctionConfig:
    """Tests for FunctionConfig"""
    
    def test_defaults(self):
        """FunctionConfig has sensible defaults"""
        config = FunctionConfig(name="test")
        
        assert config.runtime == "python3.11"
        assert config.memory_mb == 256
        assert config.timeout_seconds == 30
    
    def test_custom_values(self):
        """FunctionConfig accepts custom values"""
        config = FunctionConfig(
            name="custom",
            runtime="python3.10",
            memory_mb=512,
            timeout_seconds=60,
            environment={"KEY": "value"},
        )
        
        assert config.memory_mb == 512
        assert config.environment["KEY"] == "value"


class TestLambdaRuntime:
    """Tests for AWS Lambda runtime"""
    
    def test_create_handler(self):
        """Generates valid Lambda handler"""
        runtime = LambdaRuntime()
        handler = runtime.create_handler(SAMPLE_AGENT_CODE)
        
        assert "lambda_handler" in handler
        assert "def create_agent" in handler
        assert "asyncio.run" in handler
        assert "AetherRuntime" in handler
    
    def test_generate_deployment_sam_template(self):
        """Generates SAM template"""
        runtime = LambdaRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.AWS_LAMBDA,
            region="us-east-1",
            function_config=FunctionConfig(name="test-agent"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "template.yaml" in files
        assert "AWS::Serverless::Function" in files["template.yaml"]
        assert "test-agent" in files["template.yaml"]
    
    def test_generate_deployment_samconfig(self):
        """Generates SAM config"""
        runtime = LambdaRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.AWS_LAMBDA,
            region="eu-west-1",
            function_config=FunctionConfig(name="my-agent"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "samconfig.toml" in files
        assert "eu-west-1" in files["samconfig.toml"]
    
    def test_requirements(self):
        """Includes required packages"""
        runtime = LambdaRuntime()
        reqs = runtime.get_requirements()
        
        assert any("aethershell" in r for r in reqs)
        assert any("boto3" in r for r in reqs)


class TestAzureFunctionsRuntime:
    """Tests for Azure Functions runtime"""
    
    def test_create_handler(self):
        """Generates valid Azure Functions handler"""
        runtime = AzureFunctionsRuntime()
        handler = runtime.create_handler(SAMPLE_AGENT_CODE)
        
        assert "azure.functions" in handler
        assert "async def main" in handler
        assert "func.HttpRequest" in handler
        assert "def create_agent" in handler
    
    def test_generate_deployment_function_json(self):
        """Generates function.json"""
        runtime = AzureFunctionsRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.AZURE_FUNCTIONS,
            region="eastus",
            function_config=FunctionConfig(name="agent-func"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "function_app/function.json" in files
        function_json = json.loads(files["function_app/function.json"])
        assert function_json["bindings"][0]["type"] == "httpTrigger"
    
    def test_generate_deployment_bicep(self):
        """Generates Bicep infrastructure"""
        runtime = AzureFunctionsRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.AZURE_FUNCTIONS,
            region="westeurope",
            function_config=FunctionConfig(name="agent-func"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "deploy.bicep" in files
        assert "Microsoft.Web/sites" in files["deploy.bicep"]
    
    def test_requirements(self):
        """Includes required packages"""
        runtime = AzureFunctionsRuntime()
        reqs = runtime.get_requirements()
        
        assert any("aethershell" in r for r in reqs)
        assert any("azure-functions" in r for r in reqs)


class TestGCPFunctionsRuntime:
    """Tests for GCP Cloud Functions runtime"""
    
    def test_create_handler(self):
        """Generates valid GCP handler"""
        runtime = GCPFunctionsRuntime()
        handler = runtime.create_handler(SAMPLE_AGENT_CODE)
        
        assert "functions_framework" in handler
        assert "@functions_framework.http" in handler
        assert "def agent_handler" in handler
        assert "def create_agent" in handler
    
    def test_generate_deployment_cloudbuild(self):
        """Generates Cloud Build config"""
        runtime = GCPFunctionsRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.GCP_FUNCTIONS,
            region="us-central1",
            function_config=FunctionConfig(name="gcp-agent"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "cloudbuild.yaml" in files
        assert "gcloud" in files["cloudbuild.yaml"]
        assert "functions" in files["cloudbuild.yaml"]
    
    def test_generate_deployment_terraform(self):
        """Generates Terraform config"""
        runtime = GCPFunctionsRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.GCP_FUNCTIONS,
            region="europe-west1",
            function_config=FunctionConfig(name="gcp-agent"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "main.tf" in files
        assert "google_cloudfunctions2_function" in files["main.tf"]
    
    def test_requirements(self):
        """Includes required packages"""
        runtime = GCPFunctionsRuntime()
        reqs = runtime.get_requirements()
        
        assert any("aethershell" in r for r in reqs)
        assert any("functions-framework" in r for r in reqs)


class TestKnativeRuntime:
    """Tests for Knative/Kubernetes runtime"""
    
    def test_create_handler(self):
        """Generates valid FastAPI handler"""
        runtime = KnativeRuntime()
        handler = runtime.create_handler(SAMPLE_AGENT_CODE)
        
        assert "FastAPI" in handler
        assert "@app.post" in handler
        assert "/agent" in handler
        assert "/health" in handler
        assert "def create_agent" in handler
    
    def test_generate_deployment_dockerfile(self):
        """Generates Dockerfile"""
        runtime = KnativeRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.KNATIVE,
            region="default",
            function_config=FunctionConfig(name="k8s-agent"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "Dockerfile" in files
        assert "FROM python" in files["Dockerfile"]
        assert "uvicorn" in files["Dockerfile"]
    
    def test_generate_deployment_knative_service(self):
        """Generates Knative service manifest"""
        runtime = KnativeRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.KNATIVE,
            region="default",
            function_config=FunctionConfig(name="k8s-agent"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "knative/service.yaml" in files
        assert "serving.knative.dev" in files["knative/service.yaml"]
    
    def test_generate_deployment_k8s_manifests(self):
        """Generates Kubernetes manifests"""
        runtime = KnativeRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.KNATIVE,
            region="default",
            function_config=FunctionConfig(name="k8s-agent"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "k8s/deployment.yaml" in files
        assert "Deployment" in files["k8s/deployment.yaml"]
        assert "Service" in files["k8s/deployment.yaml"]
        assert "Ingress" in files["k8s/deployment.yaml"]
    
    def test_generate_deployment_skaffold(self):
        """Generates Skaffold config"""
        runtime = KnativeRuntime()
        config = DeploymentConfig(
            provider=CloudProvider.KNATIVE,
            region="default",
            function_config=FunctionConfig(name="k8s-agent"),
        )
        
        files = runtime.generate_deployment(config)
        
        assert "skaffold.yaml" in files
        assert "skaffold" in files["skaffold.yaml"]
    
    def test_requirements(self):
        """Includes required packages"""
        runtime = KnativeRuntime()
        reqs = runtime.get_requirements()
        
        assert any("aethershell" in r for r in reqs)
        assert any("fastapi" in r for r in reqs)
        assert any("uvicorn" in r for r in reqs)


class TestFactoryFunctions:
    """Tests for factory functions"""
    
    def test_get_runtime_lambda(self):
        """get_runtime returns Lambda runtime"""
        runtime = get_runtime(CloudProvider.AWS_LAMBDA)
        assert isinstance(runtime, LambdaRuntime)
    
    def test_get_runtime_azure(self):
        """get_runtime returns Azure runtime"""
        runtime = get_runtime(CloudProvider.AZURE_FUNCTIONS)
        assert isinstance(runtime, AzureFunctionsRuntime)
    
    def test_get_runtime_gcp(self):
        """get_runtime returns GCP runtime"""
        runtime = get_runtime(CloudProvider.GCP_FUNCTIONS)
        assert isinstance(runtime, GCPFunctionsRuntime)
    
    def test_get_runtime_knative(self):
        """get_runtime returns Knative runtime"""
        runtime = get_runtime(CloudProvider.KNATIVE)
        assert isinstance(runtime, KnativeRuntime)
    
    def test_create_handler(self):
        """create_handler generates handler code"""
        handler = create_handler(CloudProvider.AWS_LAMBDA, SAMPLE_AGENT_CODE)
        
        assert "lambda_handler" in handler
        assert "def create_agent" in handler
    
    def test_deploy_agent_creates_files(self):
        """deploy_agent generates all files"""
        config = DeploymentConfig(
            provider=CloudProvider.AWS_LAMBDA,
            region="us-east-1",
            function_config=FunctionConfig(name="test"),
        )
        
        with tempfile.TemporaryDirectory() as tmpdir:
            files = deploy_agent(config, SAMPLE_AGENT_CODE, tmpdir)
            
            assert "handler.py" in files
            assert "template.yaml" in files
            
            # Verify files were written
            assert os.path.exists(os.path.join(tmpdir, "handler.py"))
            assert os.path.exists(os.path.join(tmpdir, "template.yaml"))


class TestIntegration:
    """Integration tests for cloud deployment"""
    
    def test_full_lambda_deployment(self):
        """Full Lambda deployment workflow"""
        config = DeploymentConfig(
            provider=CloudProvider.AWS_LAMBDA,
            region="us-east-1",
            function_config=FunctionConfig(
                name="my-agent",
                memory_mb=512,
                timeout_seconds=60,
                environment={"MODEL": "gpt-4"},
                tags={"team": "ai"},
            ),
            stage="prod",
        )
        
        files = deploy_agent(config, SAMPLE_AGENT_CODE, output_dir="")
        
        # Verify handler
        assert "async def run_agent" in files["handler.py"]
        
        # Verify SAM template
        assert "512" in files["template.yaml"]  # Memory
        assert "60" in files["template.yaml"]   # Timeout
        assert "prod" in files["template.yaml"] # Stage
    
    def test_full_k8s_deployment(self):
        """Full Kubernetes deployment workflow"""
        config = DeploymentConfig(
            provider=CloudProvider.KNATIVE,
            region="default",
            function_config=FunctionConfig(
                name="k8s-agent",
                memory_mb=1024,
                concurrency=5,
            ),
            custom_domain="agent.example.com",
        )
        
        files = deploy_agent(config, SAMPLE_AGENT_CODE, output_dir="")
        
        # Verify FastAPI handler
        assert "FastAPI" in files["main.py"]
        assert "@app.post" in files["main.py"]
        
        # Verify Dockerfile
        assert "python:3.11" in files["Dockerfile"]
        
        # Verify K8s manifests
        assert "1024Mi" in files["k8s/deployment.yaml"]
        assert "agent.example.com" in files["k8s/deployment.yaml"]
