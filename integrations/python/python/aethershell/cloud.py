"""
AetherShell Cloud Platform

Deploy AetherShell agents as serverless functions:
- AWS Lambda
- Azure Functions
- Google Cloud Functions
- Kubernetes (via Knative)

Provides infrastructure-as-code templates and runtime adapters.
"""

from __future__ import annotations

import json
import os
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Callable, Dict, List, Optional, TypeVar

__all__ = [
    "CloudProvider",
    "FunctionConfig",
    "DeploymentConfig",
    "CloudRuntime",
    "LambdaRuntime",
    "AzureFunctionsRuntime",
    "GCPFunctionsRuntime",
    "KnativeRuntime",
    "deploy_agent",
    "create_handler",
]

T = TypeVar("T")


class CloudProvider(Enum):
    """Supported cloud providers"""
    AWS_LAMBDA = "aws_lambda"
    AZURE_FUNCTIONS = "azure_functions"
    GCP_FUNCTIONS = "gcp_functions"
    KNATIVE = "knative"


@dataclass
class FunctionConfig:
    """Configuration for a serverless function"""
    name: str
    runtime: str = "python3.11"
    memory_mb: int = 256
    timeout_seconds: int = 30
    environment: Dict[str, str] = field(default_factory=dict)
    vpc_config: Optional[Dict[str, Any]] = None
    concurrency: Optional[int] = None
    layers: List[str] = field(default_factory=list)
    tags: Dict[str, str] = field(default_factory=dict)


@dataclass
class DeploymentConfig:
    """Deployment configuration"""
    provider: CloudProvider
    region: str
    function_config: FunctionConfig
    api_gateway: bool = True
    cors_enabled: bool = True
    custom_domain: Optional[str] = None
    stage: str = "prod"


class CloudRuntime(ABC):
    """Base class for cloud runtime adapters"""
    
    @abstractmethod
    def create_handler(self, agent_code: str) -> str:
        """Generate handler code for the cloud platform"""
        pass
    
    @abstractmethod
    def generate_deployment(self, config: DeploymentConfig) -> Dict[str, str]:
        """Generate deployment configuration files"""
        pass
    
    @abstractmethod
    def get_requirements(self) -> List[str]:
        """Get Python requirements for the runtime"""
        pass


class LambdaRuntime(CloudRuntime):
    """AWS Lambda runtime adapter"""
    
    def create_handler(self, agent_code: str) -> str:
        """Generate Lambda handler"""
        return f'''"""
AetherShell Lambda Handler
Auto-generated - do not edit directly
"""

import json
import os
import asyncio
from aethershell import AetherRuntime, Agent

# Initialize runtime (reused across invocations)
runtime = AetherRuntime()

{agent_code}

def lambda_handler(event, context):
    """
    AWS Lambda entry point.
    
    Supports:
    - API Gateway (REST and HTTP API)
    - Direct invocation
    - SQS triggers
    """
    try:
        # Parse input based on trigger type
        if "body" in event:
            # API Gateway
            body = json.loads(event.get("body", "{{}}"))
            goal = body.get("goal", "")
            params = body.get("params", {{}})
        elif "Records" in event:
            # SQS
            record = event["Records"][0]
            body = json.loads(record.get("body", "{{}}"))
            goal = body.get("goal", "")
            params = body.get("params", {{}})
        else:
            # Direct invocation
            goal = event.get("goal", "")
            params = event.get("params", {{}})
        
        # Run agent
        result = asyncio.run(run_agent(goal, params))
        
        # Format response for API Gateway
        return {{
            "statusCode": 200,
            "headers": {{
                "Content-Type": "application/json",
                "Access-Control-Allow-Origin": "*",
            }},
            "body": json.dumps(result, default=str),
        }}
    except Exception as e:
        return {{
            "statusCode": 500,
            "headers": {{"Content-Type": "application/json"}},
            "body": json.dumps({{"error": str(e)}}),
        }}


async def run_agent(goal: str, params: dict):
    """Execute the agent with given goal"""
    agent = create_agent(runtime)
    result = await agent.run(goal)
    return {{
        "success": result.success if hasattr(result, "success") else True,
        "result": result.result if hasattr(result, "result") else result,
    }}
'''
    
    def generate_deployment(self, config: DeploymentConfig) -> Dict[str, str]:
        """Generate AWS SAM template and supporting files"""
        fc = config.function_config
        
        sam_template = f'''AWSTemplateFormatVersion: '2010-09-09'
Transform: AWS::Serverless-2016-10-31
Description: AetherShell Agent - {fc.name}

Globals:
  Function:
    Timeout: {fc.timeout_seconds}
    MemorySize: {fc.memory_mb}
    Runtime: {fc.runtime}
    Environment:
      Variables:
        AETHER_AI: "{{{{resolve:ssm:/aethershell/ai_provider}}}}"
        OPENAI_API_KEY: "{{{{resolve:ssm:/aethershell/openai_key}}}}"
{self._format_env_vars(fc.environment, 8)}

Resources:
  AgentFunction:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: {fc.name}
      CodeUri: ./
      Handler: handler.lambda_handler
      Description: AetherShell agent function
{self._format_tags(fc.tags, 6)}
      Events:
        ApiEvent:
          Type: HttpApi
          Properties:
            Path: /agent
            Method: POST
            ApiId: !Ref AgentApi
{self._format_vpc_config(fc.vpc_config, 6) if fc.vpc_config else ""}

  AgentApi:
    Type: AWS::Serverless::HttpApi
    Properties:
      StageName: {config.stage}
      CorsConfiguration:
        AllowMethods:
          - POST
          - OPTIONS
        AllowHeaders:
          - Content-Type
          - Authorization
        AllowOrigins:
          - "*"

Outputs:
  ApiEndpoint:
    Description: API Gateway endpoint URL
    Value: !Sub "https://${{AgentApi}}.execute-api.${{AWS::Region}}.amazonaws.com/{config.stage}/agent"
  FunctionArn:
    Description: Lambda function ARN
    Value: !GetAtt AgentFunction.Arn
'''
        
        samconfig = f'''version = 0.1
[default.deploy.parameters]
stack_name = "aethershell-{fc.name}"
resolve_s3 = true
s3_prefix = "aethershell-{fc.name}"
region = "{config.region}"
confirm_changeset = false
capabilities = "CAPABILITY_IAM"
'''
        
        return {
            "template.yaml": sam_template,
            "samconfig.toml": samconfig,
            "requirements.txt": "\n".join(self.get_requirements()),
        }
    
    def get_requirements(self) -> List[str]:
        return [
            "aethershell>=0.3.0",
            "boto3>=1.28.0",
        ]
    
    def _format_env_vars(self, env: Dict[str, str], indent: int) -> str:
        if not env:
            return ""
        spaces = " " * indent
        lines = [f"{spaces}{k}: {v}" for k, v in env.items()]
        return "\n".join(lines)
    
    def _format_tags(self, tags: Dict[str, str], indent: int) -> str:
        if not tags:
            return ""
        spaces = " " * indent
        lines = [f"{spaces}Tags:"]
        for k, v in tags.items():
            lines.append(f"{spaces}  {k}: {v}")
        return "\n".join(lines)
    
    def _format_vpc_config(self, vpc: Dict[str, Any], indent: int) -> str:
        if not vpc:
            return ""
        spaces = " " * indent
        return f'''{spaces}VpcConfig:
{spaces}  SubnetIds: {vpc.get("subnet_ids", [])}
{spaces}  SecurityGroupIds: {vpc.get("security_group_ids", [])}'''


class AzureFunctionsRuntime(CloudRuntime):
    """Azure Functions runtime adapter"""
    
    def create_handler(self, agent_code: str) -> str:
        """Generate Azure Functions handler"""
        return f'''"""
AetherShell Azure Functions Handler
Auto-generated - do not edit directly
"""

import json
import logging
import asyncio
import azure.functions as func
from aethershell import AetherRuntime, Agent

# Initialize runtime
runtime = AetherRuntime()

{agent_code}

async def main(req: func.HttpRequest) -> func.HttpResponse:
    """
    Azure Functions HTTP trigger entry point.
    """
    logging.info('AetherShell agent invoked')
    
    try:
        # Parse request
        try:
            body = req.get_json()
        except ValueError:
            body = {{}}
        
        goal = body.get("goal", "")
        params = body.get("params", {{}})
        
        # Run agent
        result = await run_agent(goal, params)
        
        return func.HttpResponse(
            json.dumps(result, default=str),
            status_code=200,
            mimetype="application/json",
            headers={{"Access-Control-Allow-Origin": "*"}},
        )
    except Exception as e:
        logging.error(f"Agent error: {{e}}")
        return func.HttpResponse(
            json.dumps({{"error": str(e)}}),
            status_code=500,
            mimetype="application/json",
        )


async def run_agent(goal: str, params: dict):
    """Execute the agent"""
    agent = create_agent(runtime)
    result = await agent.run(goal)
    return {{
        "success": getattr(result, "success", True),
        "result": getattr(result, "result", result),
    }}
'''
    
    def generate_deployment(self, config: DeploymentConfig) -> Dict[str, str]:
        """Generate Azure Functions configuration"""
        fc = config.function_config
        
        function_json = json.dumps({
            "bindings": [
                {
                    "authLevel": "function",
                    "type": "httpTrigger",
                    "direction": "in",
                    "name": "req",
                    "methods": ["post", "options"],
                    "route": "agent",
                },
                {
                    "type": "http",
                    "direction": "out",
                    "name": "$return",
                }
            ]
        }, indent=2)
        
        host_json = json.dumps({
            "version": "2.0",
            "logging": {
                "applicationInsights": {
                    "samplingSettings": {
                        "isEnabled": True,
                        "excludedTypes": "Request"
                    }
                }
            },
            "extensionBundle": {
                "id": "Microsoft.Azure.Functions.ExtensionBundle",
                "version": "[3.*, 4.0.0)"
            },
            "functionTimeout": f"00:{fc.timeout_seconds // 60:02d}:{fc.timeout_seconds % 60:02d}",
        }, indent=2)
        
        local_settings = json.dumps({
            "IsEncrypted": False,
            "Values": {
                "AzureWebJobsStorage": "",
                "FUNCTIONS_WORKER_RUNTIME": "python",
                **fc.environment,
            }
        }, indent=2)
        
        bicep = f'''@description('Location for all resources')
param location string = resourceGroup().location

@description('Function app name')
param functionAppName string = '{fc.name}'

resource storageAccount 'Microsoft.Storage/storageAccounts@2022-05-01' = {{
  name: '${{functionAppName}}storage'
  location: location
  sku: {{
    name: 'Standard_LRS'
  }}
  kind: 'StorageV2'
}}

resource appServicePlan 'Microsoft.Web/serverfarms@2022-03-01' = {{
  name: '${{functionAppName}}-plan'
  location: location
  sku: {{
    name: 'Y1'
    tier: 'Dynamic'
  }}
  properties: {{
    reserved: true
  }}
}}

resource functionApp 'Microsoft.Web/sites@2022-03-01' = {{
  name: functionAppName
  location: location
  kind: 'functionapp,linux'
  properties: {{
    serverFarmId: appServicePlan.id
    siteConfig: {{
      pythonVersion: '3.11'
      linuxFxVersion: 'PYTHON|3.11'
      appSettings: [
        {{
          name: 'AzureWebJobsStorage'
          value: 'DefaultEndpointsProtocol=https;AccountName=${{storageAccount.name}};AccountKey=${{storageAccount.listKeys().keys[0].value}}'
        }}
        {{
          name: 'FUNCTIONS_WORKER_RUNTIME'
          value: 'python'
        }}
        {{
          name: 'WEBSITE_RUN_FROM_PACKAGE'
          value: '1'
        }}
      ]
    }}
  }}
}}

output functionAppUrl string = 'https://${{functionApp.properties.defaultHostName}}/api/agent'
'''
        
        return {
            "function_app/__init__.py": self.create_handler(""),
            "function_app/function.json": function_json,
            "host.json": host_json,
            "local.settings.json": local_settings,
            "deploy.bicep": bicep,
            "requirements.txt": "\n".join(self.get_requirements()),
        }
    
    def get_requirements(self) -> List[str]:
        return [
            "aethershell>=0.3.0",
            "azure-functions>=1.15.0",
        ]


class GCPFunctionsRuntime(CloudRuntime):
    """Google Cloud Functions runtime adapter"""
    
    def create_handler(self, agent_code: str) -> str:
        """Generate GCP Cloud Functions handler"""
        return f'''"""
AetherShell GCP Cloud Functions Handler
Auto-generated - do not edit directly
"""

import json
import asyncio
import functions_framework
from flask import jsonify
from aethershell import AetherRuntime, Agent

# Initialize runtime
runtime = AetherRuntime()

{agent_code}

@functions_framework.http
def agent_handler(request):
    """
    GCP Cloud Functions HTTP entry point.
    """
    # Handle CORS preflight
    if request.method == "OPTIONS":
        headers = {{
            "Access-Control-Allow-Origin": "*",
            "Access-Control-Allow-Methods": "POST, OPTIONS",
            "Access-Control-Allow-Headers": "Content-Type, Authorization",
            "Access-Control-Max-Age": "3600",
        }}
        return ("", 204, headers)
    
    headers = {{"Access-Control-Allow-Origin": "*"}}
    
    try:
        # Parse request
        body = request.get_json(silent=True) or {{}}
        goal = body.get("goal", "")
        params = body.get("params", {{}})
        
        # Run agent
        result = asyncio.run(run_agent(goal, params))
        
        return (jsonify(result), 200, headers)
    except Exception as e:
        return (jsonify({{"error": str(e)}}), 500, headers)


async def run_agent(goal: str, params: dict):
    """Execute the agent"""
    agent = create_agent(runtime)
    result = await agent.run(goal)
    return {{
        "success": getattr(result, "success", True),
        "result": getattr(result, "result", result),
    }}
'''
    
    def generate_deployment(self, config: DeploymentConfig) -> Dict[str, str]:
        """Generate GCP deployment configuration"""
        fc = config.function_config
        
        cloudbuild_yaml = f'''steps:
  - name: 'gcr.io/google.com/cloudsdktool/cloud-sdk'
    entrypoint: 'gcloud'
    args:
      - 'functions'
      - 'deploy'
      - '{fc.name}'
      - '--gen2'
      - '--runtime=python311'
      - '--region={config.region}'
      - '--source=.'
      - '--entry-point=agent_handler'
      - '--trigger-http'
      - '--allow-unauthenticated'
      - '--memory={fc.memory_mb}MB'
      - '--timeout={fc.timeout_seconds}s'
{self._format_env_flags(fc.environment)}
'''
        
        terraform = f'''terraform {{
  required_providers {{
    google = {{
      source  = "hashicorp/google"
      version = "~> 4.0"
    }}
  }}
}}

provider "google" {{
  project = var.project_id
  region  = "{config.region}"
}}

variable "project_id" {{
  description = "GCP project ID"
  type        = string
}}

resource "google_storage_bucket" "function_bucket" {{
  name     = "${{var.project_id}}-{fc.name}-source"
  location = "{config.region}"
}}

resource "google_storage_bucket_object" "function_source" {{
  name   = "function-source.zip"
  bucket = google_storage_bucket.function_bucket.name
  source = "function-source.zip"
}}

resource "google_cloudfunctions2_function" "agent" {{
  name        = "{fc.name}"
  location    = "{config.region}"
  description = "AetherShell agent function"

  build_config {{
    runtime     = "python311"
    entry_point = "agent_handler"
    source {{
      storage_source {{
        bucket = google_storage_bucket.function_bucket.name
        object = google_storage_bucket_object.function_source.name
      }}
    }}
  }}

  service_config {{
    max_instance_count = {fc.concurrency or 100}
    available_memory   = "{fc.memory_mb}M"
    timeout_seconds    = {fc.timeout_seconds}
{self._format_tf_env_vars(fc.environment, 4)}
  }}
}}

resource "google_cloud_run_service_iam_member" "invoker" {{
  location = google_cloudfunctions2_function.agent.location
  service  = google_cloudfunctions2_function.agent.name
  role     = "roles/run.invoker"
  member   = "allUsers"
}}

output "function_url" {{
  value = google_cloudfunctions2_function.agent.service_config[0].uri
}}
'''
        
        return {
            "main.py": self.create_handler(""),
            "cloudbuild.yaml": cloudbuild_yaml,
            "main.tf": terraform,
            "requirements.txt": "\n".join(self.get_requirements()),
        }
    
    def get_requirements(self) -> List[str]:
        return [
            "aethershell>=0.3.0",
            "functions-framework>=3.0.0",
            "flask>=2.0.0",
        ]
    
    def _format_env_flags(self, env: Dict[str, str]) -> str:
        if not env:
            return ""
        flags = [f"      - '--set-env-vars={k}={v}'" for k, v in env.items()]
        return "\n".join(flags)
    
    def _format_tf_env_vars(self, env: Dict[str, str], indent: int) -> str:
        if not env:
            return ""
        spaces = " " * indent
        lines = [f"{spaces}environment_variables = {{"]
        for k, v in env.items():
            lines.append(f'{spaces}  {k} = "{v}"')
        lines.append(f"{spaces}}}")
        return "\n".join(lines)


class KnativeRuntime(CloudRuntime):
    """Knative/Kubernetes runtime adapter"""
    
    def create_handler(self, agent_code: str) -> str:
        """Generate Knative handler (FastAPI-based)"""
        return f'''"""
AetherShell Knative Handler
Auto-generated - do not edit directly
"""

import json
import asyncio
from contextlib import asynccontextmanager
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import Dict, Any, Optional
from aethershell import AetherRuntime, Agent

# Initialize runtime
runtime = AetherRuntime()

{agent_code}


class AgentRequest(BaseModel):
    goal: str
    params: Optional[Dict[str, Any]] = {{}}


class AgentResponse(BaseModel):
    success: bool
    result: Any


@asynccontextmanager
async def lifespan(app: FastAPI):
    # Startup: warm up the runtime
    runtime.evaluate("1 + 1")
    yield
    # Shutdown


app = FastAPI(
    title="AetherShell Agent",
    description="AI-powered agent service",
    version="0.3.0",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/health")
async def health():
    """Health check endpoint"""
    return {{"status": "healthy"}}


@app.post("/agent", response_model=AgentResponse)
async def run_agent_endpoint(request: AgentRequest):
    """Run the agent with a goal"""
    try:
        agent = create_agent(runtime)
        result = await agent.run(request.goal)
        return AgentResponse(
            success=getattr(result, "success", True),
            result=getattr(result, "result", result),
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8080)
'''
    
    def generate_deployment(self, config: DeploymentConfig) -> Dict[str, str]:
        """Generate Kubernetes/Knative manifests"""
        fc = config.function_config
        
        dockerfile = f'''FROM python:3.11-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

ENV PORT=8080
EXPOSE 8080

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8080"]
'''
        
        knative_service = f'''apiVersion: serving.knative.dev/v1
kind: Service
metadata:
  name: {fc.name}
  labels:
    app: aethershell-agent
spec:
  template:
    metadata:
      annotations:
        autoscaling.knative.dev/minScale: "0"
        autoscaling.knative.dev/maxScale: "{fc.concurrency or 10}"
    spec:
      containerConcurrency: 10
      timeoutSeconds: {fc.timeout_seconds}
      containers:
        - image: ${{IMAGE_URL}}
          ports:
            - containerPort: 8080
          resources:
            limits:
              memory: {fc.memory_mb}Mi
              cpu: "1"
          env:
{self._format_k8s_env(fc.environment, 12)}
          readinessProbe:
            httpGet:
              path: /health
            initialDelaySeconds: 5
'''
        
        k8s_deployment = f'''apiVersion: apps/v1
kind: Deployment
metadata:
  name: {fc.name}
  labels:
    app: aethershell-agent
spec:
  replicas: 1
  selector:
    matchLabels:
      app: {fc.name}
  template:
    metadata:
      labels:
        app: {fc.name}
    spec:
      containers:
        - name: agent
          image: ${{IMAGE_URL}}
          ports:
            - containerPort: 8080
          resources:
            requests:
              memory: "{fc.memory_mb // 2}Mi"
              cpu: "250m"
            limits:
              memory: "{fc.memory_mb}Mi"
              cpu: "1"
          env:
{self._format_k8s_env(fc.environment, 12)}
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 10
          readinessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 5
---
apiVersion: v1
kind: Service
metadata:
  name: {fc.name}
spec:
  selector:
    app: {fc.name}
  ports:
    - port: 80
      targetPort: 8080
  type: ClusterIP
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {fc.name}
  annotations:
    kubernetes.io/ingress.class: nginx
spec:
  rules:
    - host: {config.custom_domain or f"{fc.name}.example.com"}
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: {fc.name}
                port:
                  number: 80
'''
        
        skaffold = f'''apiVersion: skaffold/v4beta6
kind: Config
metadata:
  name: {fc.name}
build:
  artifacts:
    - image: {fc.name}
      docker:
        dockerfile: Dockerfile
deploy:
  kubectl:
    manifests:
      - k8s/*.yaml
'''
        
        return {
            "main.py": self.create_handler(""),
            "Dockerfile": dockerfile,
            "knative/service.yaml": knative_service,
            "k8s/deployment.yaml": k8s_deployment,
            "skaffold.yaml": skaffold,
            "requirements.txt": "\n".join(self.get_requirements()),
        }
    
    def get_requirements(self) -> List[str]:
        return [
            "aethershell>=0.3.0",
            "fastapi>=0.100.0",
            "uvicorn>=0.23.0",
            "pydantic>=2.0.0",
        ]
    
    def _format_k8s_env(self, env: Dict[str, str], indent: int) -> str:
        if not env:
            spaces = " " * indent
            return f"{spaces}- name: AETHER_AI\n{spaces}  value: openai"
        
        lines = []
        spaces = " " * indent
        for k, v in env.items():
            lines.append(f"{spaces}- name: {k}")
            lines.append(f"{spaces}  value: \"{v}\"")
        return "\n".join(lines)


# ============================================================================
# Factory Functions
# ============================================================================

def get_runtime(provider: CloudProvider) -> CloudRuntime:
    """Get the appropriate runtime for a cloud provider"""
    runtimes = {
        CloudProvider.AWS_LAMBDA: LambdaRuntime(),
        CloudProvider.AZURE_FUNCTIONS: AzureFunctionsRuntime(),
        CloudProvider.GCP_FUNCTIONS: GCPFunctionsRuntime(),
        CloudProvider.KNATIVE: KnativeRuntime(),
    }
    return runtimes[provider]


def create_handler(
    provider: CloudProvider,
    agent_code: str,
) -> str:
    """
    Create a cloud function handler for an agent.
    
    Args:
        provider: Target cloud provider
        agent_code: Agent creation code (should define create_agent function)
        
    Returns:
        Handler code as string
    """
    runtime = get_runtime(provider)
    return runtime.create_handler(agent_code)


def deploy_agent(
    config: DeploymentConfig,
    agent_code: str,
    output_dir: str = ".",
) -> Dict[str, str]:
    """
    Generate deployment files for an agent.
    
    Args:
        config: Deployment configuration
        agent_code: Agent creation code
        output_dir: Directory to write files
        
    Returns:
        Dictionary of filename -> content
    """
    import os
    
    runtime = get_runtime(config.provider)
    files = runtime.generate_deployment(config)
    
    # Add the handler with agent code
    if config.provider == CloudProvider.AWS_LAMBDA:
        files["handler.py"] = runtime.create_handler(agent_code)
    elif config.provider == CloudProvider.AZURE_FUNCTIONS:
        files["function_app/__init__.py"] = runtime.create_handler(agent_code)
    elif config.provider == CloudProvider.GCP_FUNCTIONS:
        files["main.py"] = runtime.create_handler(agent_code)
    elif config.provider == CloudProvider.KNATIVE:
        files["main.py"] = runtime.create_handler(agent_code)
    
    # Write files if output_dir specified
    if output_dir:
        for filename, content in files.items():
            filepath = os.path.join(output_dir, filename)
            os.makedirs(os.path.dirname(filepath) or ".", exist_ok=True)
            with open(filepath, "w") as f:
                f.write(content)
    
    return files
