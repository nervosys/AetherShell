/**
 * AetherShell Agent API - WebSocket Integration Example
 * 
 * This example demonstrates bidirectional real-time communication with
 * AetherShell's Agent API using WebSockets. This is ideal for:
 * - Multi-agent systems with inter-agent communication
 * - Real-time collaborative AI workflows
 * - Long-running tasks with streaming updates
 * 
 * Start the Agent API server:
 *   ae --agent-api
 */

// =============================================================================
// WebSocket Client Class
// =============================================================================

interface WsClientMessage {
  type: 'execute' | 'subscribe' | 'unsubscribe' | 'ping' | 'register' | 'agent_message' | 'broadcast';
  [key: string]: unknown;
}

interface WsServerMessage {
  type: 'response' | 'stream' | 'channel' | 'pong' | 'error' | 'agent_message' | 'registered' | 'agents';
  [key: string]: unknown;
}

type MessageHandler = (message: WsServerMessage) => void;

class AetherShellWebSocket {
  private ws: WebSocket | null = null;
  private messageHandlers: Map<string, MessageHandler[]> = new Map();
  private pendingRequests: Map<string, { resolve: Function; reject: Function }> = new Map();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;

  constructor(
    private url: string = 'ws://localhost:3002/api/v1/ws',
    private agentId?: string,
    private capabilities: string[] = []
  ) {}

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.url);

      this.ws.onopen = async () => {
        console.log('WebSocket connected');
        this.reconnectAttempts = 0;

        // Register as agent if agentId provided
        if (this.agentId) {
          await this.register(this.agentId, this.capabilities);
        }
        resolve();
      };

      this.ws.onmessage = (event) => {
        const message: WsServerMessage = JSON.parse(event.data);
        this.handleMessage(message);
      };

      this.ws.onerror = (error) => {
        console.error('WebSocket error:', error);
        reject(error);
      };

      this.ws.onclose = () => {
        console.log('WebSocket closed');
        this.attemptReconnect();
      };
    });
  }

  private handleMessage(message: WsServerMessage): void {
    // Handle pending request responses
    if (message.type === 'response' && 'id' in message) {
      const pending = this.pendingRequests.get(message.id as string);
      if (pending) {
        this.pendingRequests.delete(message.id as string);
        pending.resolve(message);
      }
    }

    // Notify all handlers for this message type
    const handlers = this.messageHandlers.get(message.type) || [];
    handlers.forEach((handler) => handler(message));

    // Also notify 'all' handlers
    const allHandlers = this.messageHandlers.get('all') || [];
    allHandlers.forEach((handler) => handler(message));
  }

  private attemptReconnect(): void {
    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      this.reconnectAttempts++;
      console.log(`Reconnecting (attempt ${this.reconnectAttempts})...`);
      setTimeout(() => this.connect(), this.reconnectDelay * this.reconnectAttempts);
    }
  }

  private send(message: WsClientMessage): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    } else {
      throw new Error('WebSocket not connected');
    }
  }

  // Register message handler
  on(type: string, handler: MessageHandler): void {
    if (!this.messageHandlers.has(type)) {
      this.messageHandlers.set(type, []);
    }
    this.messageHandlers.get(type)!.push(handler);
  }

  // Remove message handler
  off(type: string, handler: MessageHandler): void {
    const handlers = this.messageHandlers.get(type);
    if (handlers) {
      const index = handlers.indexOf(handler);
      if (index !== -1) {
        handlers.splice(index, 1);
      }
    }
  }

  // Register as an agent
  async register(agentId: string, capabilities: string[] = []): Promise<void> {
    this.send({
      type: 'register',
      agent_id: agentId,
      capabilities,
    });
  }

  // Execute a request and wait for response
  async execute(request: Record<string, unknown>): Promise<WsServerMessage> {
    const id = `req-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });

      // Set timeout
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error('Request timeout'));
        }
      }, 30000);

      this.send({
        type: 'execute',
        id,
        request,
      });
    });
  }

  // Send message to another agent
  sendToAgent(targetAgentId: string, payload: unknown): void {
    this.send({
      type: 'agent_message',
      to: targetAgentId,
      payload,
    });
  }

  // Broadcast to a channel
  broadcast(channel: string, payload: unknown): void {
    this.send({
      type: 'broadcast',
      channel,
      payload,
    });
  }

  // Subscribe to a channel
  subscribe(channel: string): void {
    this.send({
      type: 'subscribe',
      channel,
    });
  }

  // Unsubscribe from a channel
  unsubscribe(channel: string): void {
    this.send({
      type: 'unsubscribe',
      channel,
    });
  }

  // Ping for keepalive
  ping(): void {
    this.send({
      type: 'ping',
      id: Date.now().toString(),
    });
  }

  // Close the connection
  close(): void {
    this.ws?.close();
  }
}

// =============================================================================
// Multi-Agent Example: Code Review System
// =============================================================================

async function codeReviewExample(): Promise<void> {
  console.log('=== Multi-Agent Code Review System ===\n');

  // Create three specialized agents
  const coordinator = new AetherShellWebSocket(
    'ws://localhost:3002/api/v1/ws',
    'coordinator',
    ['orchestration', 'task-distribution']
  );

  const analyzer = new AetherShellWebSocket(
    'ws://localhost:3002/api/v1/ws',
    'analyzer',
    ['code-analysis', 'complexity-check']
  );

  const securityScanner = new AetherShellWebSocket(
    'ws://localhost:3002/api/v1/ws',
    'security-scanner',
    ['security-scan', 'vulnerability-check']
  );

  // Connect all agents
  await Promise.all([
    coordinator.connect(),
    analyzer.connect(),
    securityScanner.connect(),
  ]);

  console.log('All agents connected!\n');

  // Set up message handlers
  analyzer.on('agent_message', async (msg) => {
    console.log('[Analyzer] Received task from:', (msg as any).from);
    const task = (msg as any).payload;

    // Perform analysis
    const result = await analyzer.execute({
      action: 'eval',
      code: `{ complexity: "low", lines: 42, functions: 3, issues: [] }`,
    });

    // Send results back to coordinator
    analyzer.sendToAgent('coordinator', {
      type: 'analysis_complete',
      task_id: task.id,
      result: (result as any).response?.result,
    });
  });

  securityScanner.on('agent_message', async (msg) => {
    console.log('[Security Scanner] Received task from:', (msg as any).from);
    const task = (msg as any).payload;

    // Perform security scan
    const result = await securityScanner.execute({
      action: 'eval',
      code: `{ vulnerabilities: 0, warnings: 1, severity: "low" }`,
    });

    // Send results back to coordinator
    securityScanner.sendToAgent('coordinator', {
      type: 'security_complete',
      task_id: task.id,
      result: (result as any).response?.result,
    });
  });

  // Coordinator receives results
  let analysisResult: any = null;
  let securityResult: any = null;

  coordinator.on('agent_message', (msg) => {
    const payload = (msg as any).payload;
    console.log('[Coordinator] Received result:', payload.type);

    if (payload.type === 'analysis_complete') {
      analysisResult = payload.result;
    } else if (payload.type === 'security_complete') {
      securityResult = payload.result;
    }

    // Check if all results received
    if (analysisResult && securityResult) {
      console.log('\n=== Code Review Complete ===');
      console.log('Analysis:', analysisResult);
      console.log('Security:', securityResult);
    }
  });

  // Coordinator distributes tasks
  console.log('[Coordinator] Distributing review tasks...\n');

  coordinator.sendToAgent('analyzer', {
    id: 'task-1',
    type: 'analyze',
    code: 'function example() { return 42; }',
  });

  coordinator.sendToAgent('security-scanner', {
    id: 'task-2',
    type: 'scan',
    code: 'function example() { return 42; }',
  });

  // Wait for results
  await new Promise((resolve) => setTimeout(resolve, 3000));

  // Clean up
  coordinator.close();
  analyzer.close();
  securityScanner.close();
}

// =============================================================================
// Real-time Collaboration Example
// =============================================================================

async function collaborationExample(): Promise<void> {
  console.log('=== Real-time Collaboration Example ===\n');

  const client = new AetherShellWebSocket(
    'ws://localhost:3002/api/v1/ws',
    'collaborator-1',
    ['eval', 'pipeline']
  );

  await client.connect();

  // Subscribe to updates channel
  client.subscribe('code-updates');
  client.subscribe('results');

  // Handle channel messages
  client.on('channel', (msg) => {
    const { channel, payload } = msg as any;
    console.log(`[${channel}] Received:`, payload);
  });

  // Execute some commands
  const result1 = await client.execute({
    action: 'eval',
    code: '[1, 2, 3] | map(fn(x) => x * 2)',
  });
  console.log('Pipeline result:', (result1 as any).response?.result);

  // Broadcast result to other collaborators
  client.broadcast('results', {
    user: 'collaborator-1',
    code: '[1, 2, 3] | map(fn(x) => x * 2)',
    result: (result1 as any).response?.result,
  });

  // Keep connection alive
  setInterval(() => client.ping(), 30000);

  // Wait a bit then close
  await new Promise((resolve) => setTimeout(resolve, 2000));
  client.close();
}

// =============================================================================
// Task Queue Example
// =============================================================================

async function taskQueueExample(): Promise<void> {
  console.log('=== Task Queue Example ===\n');

  const worker = new AetherShellWebSocket(
    'ws://localhost:3002/api/v1/ws',
    'worker-1',
    ['file-processing', 'data-transform']
  );

  await worker.connect();

  // Subscribe to task channel
  worker.subscribe('tasks');

  // Handle incoming tasks
  worker.on('channel', async (msg) => {
    const task = (msg as any).payload;
    console.log('[Worker] Processing task:', task.id);

    // Execute the task
    const result = await worker.execute(task.request);
    console.log('[Worker] Task complete:', (result as any).response?.result);

    // Broadcast completion
    worker.broadcast('task-complete', {
      task_id: task.id,
      worker: 'worker-1',
      result: (result as any).response?.result,
    });
  });

  // Simulate receiving a task
  worker.broadcast('tasks', {
    id: 'task-001',
    request: {
      action: 'eval',
      code: '{ processed: true, items: 100 }',
    },
  });

  await new Promise((resolve) => setTimeout(resolve, 2000));
  worker.close();
}

// =============================================================================
// Main
// =============================================================================

async function main(): Promise<void> {
  console.log('AetherShell WebSocket Integration Examples\n');
  console.log('='.repeat(60));

  // Check if server is running
  try {
    const ws = new WebSocket('ws://localhost:3002/api/v1/ws');
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => {
        ws.close();
        resolve();
      };
      ws.onerror = reject;
      setTimeout(() => reject(new Error('Connection timeout')), 5000);
    });
  } catch (e) {
    console.error('Error: Agent API not running on localhost:3002');
    console.log('Start it with: ae --agent-api');
    process.exit(1);
  }

  console.log('Agent API WebSocket is available!\n');

  // Run examples
  try {
    await collaborationExample();
    console.log('\n');
    await taskQueueExample();
    console.log('\n');
    // Uncomment to run multi-agent example (requires multiple connections)
    // await codeReviewExample();
  } catch (e) {
    console.error('Example error:', e);
  }
}

// Export for use as a module
export { AetherShellWebSocket };

// Run if executed directly
main().catch(console.error);
