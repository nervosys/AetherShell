import { create } from 'zustand'
import { subscribeWithSelector } from 'zustand/middleware'

// Types
export interface Agent {
    id: string
    name: string
    capabilities: string[]
    status: 'online' | 'offline' | 'busy'
    connectedAt: number
    lastActivity: number
    metrics: {
        requestsHandled: number
        averageLatency: number
        errorRate: number
    }
}

export interface Workflow {
    id: string
    name: string
    status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'
    currentStep: number
    totalSteps: number
    startedAt: number
    completedAt?: number
    error?: string
    steps: WorkflowStep[]
}

export interface WorkflowStep {
    name: string
    status: 'pending' | 'running' | 'completed' | 'failed' | 'skipped'
    startedAt?: number
    completedAt?: number
    output?: unknown
    error?: string
}

export interface MetricPoint {
    timestamp: number
    value: number
}

export interface MetricSeries {
    name: string
    type: 'counter' | 'gauge' | 'histogram'
    unit?: string
    points: MetricPoint[]
    current: number
}

export interface Task {
    id: string
    name: string
    status: 'pending' | 'assigned' | 'running' | 'completed' | 'failed'
    assignedTo?: string
    priority: number
    createdAt: number
}

// WebSocket connection state
export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error'

interface DashboardState {
    // Connection
    connectionStatus: ConnectionStatus
    ws: WebSocket | null

    // Agents
    agents: Agent[]
    selectedAgentId: string | null

    // Workflows
    workflows: Workflow[]
    selectedWorkflowId: string | null

    // Tasks
    tasks: Task[]

    // Metrics
    metrics: Map<string, MetricSeries>

    // Activity log
    activityLog: ActivityEntry[]

    // Actions
    connect: () => void
    disconnect: () => void
    selectAgent: (id: string | null) => void
    selectWorkflow: (id: string | null) => void
    executeCommand: (command: string) => Promise<unknown>
    createWorkflow: (workflow: Partial<Workflow>) => Promise<string>
    cancelWorkflow: (id: string) => Promise<void>
    publishAgent: (agentDef: AgentDefinition) => Promise<void>
    searchMarketplace: (query: string, category?: string) => Promise<MarketplaceAgent[]>
    installMarketplaceAgent: (name: string, version?: string) => Promise<void>
    uninstallMarketplaceAgent: (name: string) => Promise<void>
}

export interface ActivityEntry {
    id: string
    timestamp: number
    type: 'agent' | 'workflow' | 'task' | 'error' | 'info'
    message: string
    details?: unknown
}

export interface AgentDefinition {
    name: string
    description: string
    systemPrompt: string
    tools: string[]
    model?: string
}

export interface MarketplaceAgent {
    id: string
    name: string
    description: string
    author: string
    version: string
    downloads: number
    stars: number
    forks: number
    tags: string[]
    updatedAt: number
    verified: boolean
    installed?: boolean
}

// Store implementation
export const useDashboardStore = create<DashboardState>()(
    subscribeWithSelector((set, get) => ({
        // Initial state
        connectionStatus: 'disconnected',
        ws: null,
        agents: [],
        selectedAgentId: null,
        workflows: [],
        selectedWorkflowId: null,
        tasks: [],
        metrics: new Map(),
        activityLog: [],

        // Actions
        connect: () => {
            const { ws, connectionStatus } = get()
            if (ws || connectionStatus === 'connecting') return

            set({ connectionStatus: 'connecting' })

            const wsUrl = `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/api/v1/ws`
            const socket = new WebSocket(wsUrl)

            socket.onopen = () => {
                set({ connectionStatus: 'connected', ws: socket })
                addActivity(set, 'info', 'Connected to AetherShell server')

                // Subscribe to updates
                socket.send(JSON.stringify({ type: 'subscribe', channel: 'agents' }))
                socket.send(JSON.stringify({ type: 'subscribe', channel: 'workflows' }))
                socket.send(JSON.stringify({ type: 'subscribe', channel: 'metrics' }))
            }

            socket.onmessage = (event) => {
                try {
                    const message = JSON.parse(event.data)
                    handleMessage(set, get, message)
                } catch {
                    console.error('Failed to parse WebSocket message')
                }
            }

            socket.onclose = () => {
                set({ connectionStatus: 'disconnected', ws: null })
                addActivity(set, 'info', 'Disconnected from server')

                // Attempt reconnection after 5 seconds
                setTimeout(() => {
                    const { connectionStatus } = get()
                    if (connectionStatus === 'disconnected') {
                        get().connect()
                    }
                }, 5000)
            }

            socket.onerror = () => {
                set({ connectionStatus: 'error' })
                addActivity(set, 'error', 'WebSocket connection error')
            }

            set({ ws: socket })
        },

        disconnect: () => {
            const { ws } = get()
            if (ws) {
                ws.close()
                set({ ws: null, connectionStatus: 'disconnected' })
            }
        },

        selectAgent: (id) => set({ selectedAgentId: id }),
        selectWorkflow: (id) => set({ selectedWorkflowId: id }),

        executeCommand: async (command: string) => {
            const { ws } = get()
            if (!ws || ws.readyState !== WebSocket.OPEN) {
                throw new Error('Not connected to server')
            }

            const id = crypto.randomUUID()

            return new Promise((resolve, reject) => {
                const timeout = setTimeout(() => {
                    reject(new Error('Command timeout'))
                }, 30000)

                const handler = (event: MessageEvent) => {
                    try {
                        const message = JSON.parse(event.data)
                        if (message.type === 'response' && message.id === id) {
                            clearTimeout(timeout)
                            ws.removeEventListener('message', handler)
                            if (message.response.success) {
                                resolve(message.response.result)
                            } else {
                                reject(new Error(message.response.error || 'Command failed'))
                            }
                        }
                    } catch {
                        // Ignore parse errors for other messages
                    }
                }

                ws.addEventListener('message', handler)
                ws.send(JSON.stringify({
                    type: 'execute',
                    id,
                    request: { type: 'eval', code: command }
                }))
            })
        },

        createWorkflow: async (workflow) => {
            const response = await fetch('/api/v1/orchestration/workflows', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(workflow),
            })
            if (!response.ok) throw new Error('Failed to create workflow')
            const data = await response.json()
            addActivity(set, 'workflow', `Created workflow: ${workflow.name}`)
            return data.id
        },

        cancelWorkflow: async (id) => {
            await fetch(`/api/v1/orchestration/workflows/${id}/cancel`, {
                method: 'POST',
            })
            addActivity(set, 'workflow', `Cancelled workflow: ${id}`)
        },

        publishAgent: async (agentDef) => {
            const response = await fetch('/api/v1/marketplace/publish', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(agentDef),
            })
            if (!response.ok) throw new Error('Failed to publish agent')
            addActivity(set, 'agent', `Published agent: ${agentDef.name}`)
        },

        searchMarketplace: async (query: string, category?: string) => {
            const params = new URLSearchParams()
            if (query) params.set('q', query)
            if (category && category !== 'all') params.set('category', category)
            const response = await fetch(`/api/v1/marketplace/search?${params}`)
            if (!response.ok) throw new Error('Marketplace search failed')
            const data = await response.json()
            return (data.agents || []) as MarketplaceAgent[]
        },

        installMarketplaceAgent: async (name: string, version?: string) => {
            const response = await fetch('/api/v1/marketplace/install', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ name, version }),
            })
            if (!response.ok) {
                const data = await response.json().catch(() => ({}))
                throw new Error(data.error || 'Failed to install agent')
            }
            addActivity(set, 'agent', `Installed agent: ${name}`)
        },

        uninstallMarketplaceAgent: async (name: string) => {
            const response = await fetch('/api/v1/marketplace/uninstall', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ name }),
            })
            if (!response.ok) {
                const data = await response.json().catch(() => ({}))
                throw new Error(data.error || 'Failed to uninstall agent')
            }
            addActivity(set, 'agent', `Uninstalled agent: ${name}`)
        },
    }))
)

// Helper functions
function addActivity(
    set: (partial: Partial<DashboardState> | ((state: DashboardState) => Partial<DashboardState>)) => void,
    type: ActivityEntry['type'],
    message: string,
    details?: unknown
) {
    set((state: DashboardState) => ({
        activityLog: [
            {
                id: crypto.randomUUID(),
                timestamp: Date.now(),
                type,
                message,
                details,
            },
            ...state.activityLog.slice(0, 99), // Keep last 100 entries
        ],
    }))
}

function handleMessage(
    set: (partial: Partial<DashboardState> | ((state: DashboardState) => Partial<DashboardState>)) => void,
    _get: () => DashboardState,
    message: { type: string;[key: string]: unknown }
) {
    switch (message.type) {
        case 'agents':
            set({ agents: message.agents as Agent[] })
            break

        case 'agent_connected':
            set((state) => ({
                agents: [...state.agents, message.agent as Agent]
            }))
            addActivity(set, 'agent', `Agent connected: ${(message.agent as Agent).name}`)
            break

        case 'agent_disconnected':
            set((state) => ({
                agents: state.agents.filter(a => a.id !== message.agentId)
            }))
            addActivity(set, 'agent', `Agent disconnected: ${message.agentId}`)
            break

        case 'workflow_update':
            set((state) => ({
                workflows: state.workflows.map(w =>
                    w.id === (message.workflow as Workflow).id ? (message.workflow as Workflow) : w
                )
            }))
            break

        case 'workflow_created':
            set((state) => ({
                workflows: [message.workflow as Workflow, ...state.workflows]
            }))
            addActivity(set, 'workflow', `Workflow started: ${(message.workflow as Workflow).name}`)
            break

        case 'metric':
            set((state) => {
                const metrics = new Map(state.metrics)
                const series = metrics.get(message.name as string) || {
                    name: message.name as string,
                    type: message.metricType as 'counter' | 'gauge' | 'histogram',
                    unit: message.unit as string | undefined,
                    points: [],
                    current: 0,
                }
                series.points.push({
                    timestamp: Date.now(),
                    value: message.value as number,
                })
                series.current = message.value as number
                // Keep last 1000 points
                if (series.points.length > 1000) {
                    series.points = series.points.slice(-1000)
                }
                metrics.set(message.name as string, series)
                return { metrics }
            })
            break

        case 'task_update':
            set((state) => ({
                tasks: state.tasks.map(t =>
                    t.id === (message.task as Task).id ? (message.task as Task) : t
                )
            }))
            break

        case 'tasks':
            set({ tasks: message.tasks as Task[] })
            break

        case 'error':
            addActivity(set, 'error', message.message as string)
            break
    }
}

// Selectors
export const useConnectionStatus = () => useDashboardStore(state => state.connectionStatus)
export const useAgents = () => useDashboardStore(state => state.agents)
export const useSelectedAgent = () => useDashboardStore(state =>
    state.agents.find(a => a.id === state.selectedAgentId)
)
export const useWorkflows = () => useDashboardStore(state => state.workflows)
export const useSelectedWorkflow = () => useDashboardStore(state =>
    state.workflows.find(w => w.id === state.selectedWorkflowId)
)
export const useTasks = () => useDashboardStore(state => state.tasks)
export const useMetrics = () => useDashboardStore(state => state.metrics)
export const useActivityLog = () => useDashboardStore(state => state.activityLog)
