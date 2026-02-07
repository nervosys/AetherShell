import { useEffect } from 'react'
import {
    Bot,
    GitBranch,
    Activity,
    Clock,
    TrendingUp,
    AlertCircle,
    CheckCircle2
} from 'lucide-react'
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, AreaChart, Area } from 'recharts'
import { format } from 'date-fns'
import { Card, StatCard, Badge } from '../components/ui'
import { useAgents, useWorkflows, useMetrics, useActivityLog, useDashboardStore } from '../store'

// Generate time-series data from real metrics or mock
function generateChartData(metrics: Map<string, { points: { timestamp: number; value: number }[] }>) {
    const requestSeries = metrics.get('requests')
    if (requestSeries && requestSeries.points.length > 0) {
        return requestSeries.points.slice(-24).map(p => ({
            time: format(new Date(p.timestamp), 'HH:mm'),
            requests: Math.round(p.value),
            latency: 0,
        }))
    }
    return Array.from({ length: 24 }, (_, i) => ({
        time: format(new Date(Date.now() - (23 - i) * 3600000), 'HH:mm'),
        requests: Math.floor(Math.random() * 100) + 50,
        latency: Math.floor(Math.random() * 200) + 50,
    }))
}

function generateAgentActivityData(agents: { status: string }[]) {
    const online = agents.filter(a => a.status === 'online').length
    const busy = agents.filter(a => a.status === 'busy').length
    const idle = agents.filter(a => a.status !== 'busy' && a.status === 'online').length
    if (agents.length > 0) {
        return Array.from({ length: 12 }, (_, i) => ({
            time: format(new Date(Date.now() - (11 - i) * 300000), 'HH:mm'),
            active: online + busy,
            idle: idle,
        }))
    }
    return Array.from({ length: 12 }, (_, i) => ({
        time: format(new Date(Date.now() - (11 - i) * 300000), 'HH:mm'),
        active: Math.floor(Math.random() * 10) + 5,
        idle: Math.floor(Math.random() * 5) + 2,
    }))
}

export default function Dashboard() {
    const agents = useAgents()
    const workflows = useWorkflows()
    const metrics = useMetrics()
    const activityLog = useActivityLog()
    const connect = useDashboardStore(state => state.connect)

    // Auto-connect on mount
    useEffect(() => {
        connect()
    }, [connect])

    const activeAgents = agents.filter(a => a.status === 'online' || a.status === 'busy').length
    const runningWorkflows = workflows.filter(w => w.status === 'running').length
    const completedToday = workflows.filter(w =>
        w.status === 'completed' &&
        w.completedAt &&
        new Date(w.completedAt).toDateString() === new Date().toDateString()
    ).length

    const requestsData = generateChartData(metrics)
    const agentActivityData = generateAgentActivityData(agents)

    return (
        <div className="space-y-6 animate-fade-in">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-2xl font-bold text-text">Dashboard</h1>
                    <p className="text-subtext0 mt-1">Monitor your AetherShell infrastructure</p>
                </div>
                <div className="flex items-center gap-2 text-sm text-subtext0">
                    <Clock size={16} />
                    <span>{format(new Date(), 'PPpp')}</span>
                </div>
            </div>

            {/* Stats Grid */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <StatCard
                    label="Active Agents"
                    value={activeAgents}
                    icon={<Bot size={24} />}
                    color="blue"
                    change={{ value: agents.length > 0 ? agents.length : 0, label: 'total registered' }}
                />
                <StatCard
                    label="Running Workflows"
                    value={runningWorkflows}
                    icon={<GitBranch size={24} />}
                    color="mauve"
                    change={{ value: workflows.length, label: 'total' }}
                />
                <StatCard
                    label="Completed Today"
                    value={completedToday}
                    icon={<CheckCircle2 size={24} />}
                    color="green"
                    change={{ value: completedToday, label: 'today' }}
                />
                <StatCard
                    label="Avg Response Time"
                    value={metrics.get('latency') ? `${Math.round(metrics.get('latency')!.current)}ms` : '—'}
                    icon={<Activity size={24} />}
                    color="yellow"
                    change={{ value: 0, label: 'current' }}
                />
            </div>

            {/* Charts Row */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                {/* Requests Chart */}
                <Card title="Request Volume (24h)">
                    <div className="h-64">
                        <ResponsiveContainer width="100%" height="100%">
                            <AreaChart data={requestsData}>
                                <defs>
                                    <linearGradient id="requestsGradient" x1="0" y1="0" x2="0" y2="1">
                                        <stop offset="5%" stopColor="#cba6f7" stopOpacity={0.3} />
                                        <stop offset="95%" stopColor="#cba6f7" stopOpacity={0} />
                                    </linearGradient>
                                </defs>
                                <XAxis
                                    dataKey="time"
                                    stroke="#6c7086"
                                    fontSize={12}
                                    tickLine={false}
                                />
                                <YAxis
                                    stroke="#6c7086"
                                    fontSize={12}
                                    tickLine={false}
                                    axisLine={false}
                                />
                                <Tooltip
                                    contentStyle={{
                                        backgroundColor: '#313244',
                                        border: '1px solid #45475a',
                                        borderRadius: '8px',
                                        color: '#cdd6f4',
                                    }}
                                />
                                <Area
                                    type="monotone"
                                    dataKey="requests"
                                    stroke="#cba6f7"
                                    strokeWidth={2}
                                    fill="url(#requestsGradient)"
                                />
                            </AreaChart>
                        </ResponsiveContainer>
                    </div>
                </Card>

                {/* Agent Activity Chart */}
                <Card title="Agent Activity">
                    <div className="h-64">
                        <ResponsiveContainer width="100%" height="100%">
                            <LineChart data={agentActivityData}>
                                <XAxis
                                    dataKey="time"
                                    stroke="#6c7086"
                                    fontSize={12}
                                    tickLine={false}
                                />
                                <YAxis
                                    stroke="#6c7086"
                                    fontSize={12}
                                    tickLine={false}
                                    axisLine={false}
                                />
                                <Tooltip
                                    contentStyle={{
                                        backgroundColor: '#313244',
                                        border: '1px solid #45475a',
                                        borderRadius: '8px',
                                        color: '#cdd6f4',
                                    }}
                                />
                                <Line
                                    type="monotone"
                                    dataKey="active"
                                    stroke="#a6e3a1"
                                    strokeWidth={2}
                                    dot={false}
                                    name="Active"
                                />
                                <Line
                                    type="monotone"
                                    dataKey="idle"
                                    stroke="#89b4fa"
                                    strokeWidth={2}
                                    dot={false}
                                    name="Idle"
                                />
                            </LineChart>
                        </ResponsiveContainer>
                    </div>
                </Card>
            </div>

            {/* Bottom Row */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                {/* Recent Workflows */}
                <Card title="Recent Workflows" className="lg:col-span-2">
                    <div className="space-y-3">
                        {(workflows.length > 0 ? workflows.slice(0, 5) : mockWorkflows).map((workflow) => (
                            <div
                                key={workflow.id}
                                className="flex items-center justify-between p-3 bg-surface1/50 rounded-lg"
                            >
                                <div className="flex items-center gap-3">
                                    <div className={`w-2 h-2 rounded-full ${getStatusColor(workflow.status)}`} />
                                    <div>
                                        <p className="font-medium text-text">{workflow.name}</p>
                                        <p className="text-xs text-subtext0">
                                            Step {workflow.currentStep}/{workflow.totalSteps}
                                        </p>
                                    </div>
                                </div>
                                <Badge variant={getStatusVariant(workflow.status)}>
                                    {workflow.status}
                                </Badge>
                            </div>
                        ))}
                    </div>
                </Card>

                {/* Activity Log */}
                <Card title="Activity Log">
                    <div className="space-y-3 max-h-80 overflow-auto">
                        {(activityLog.length > 0 ? activityLog.slice(0, 10) : mockActivity).map((entry) => (
                            <div key={entry.id} className="flex gap-3 text-sm">
                                <div className={`mt-1 ${getActivityColor(entry.type)}`}>
                                    {getActivityIcon(entry.type)}
                                </div>
                                <div className="flex-1 min-w-0">
                                    <p className="text-text truncate">{entry.message}</p>
                                    <p className="text-xs text-subtext0">
                                        {format(entry.timestamp, 'HH:mm:ss')}
                                    </p>
                                </div>
                            </div>
                        ))}
                    </div>
                </Card>
            </div>
        </div>
    )
}

// Helper functions
function getStatusColor(status: string): string {
    switch (status) {
        case 'completed': return 'bg-green'
        case 'running': return 'bg-blue animate-pulse'
        case 'failed': return 'bg-red'
        case 'pending': return 'bg-yellow'
        default: return 'bg-surface2'
    }
}

function getStatusVariant(status: string): 'success' | 'warning' | 'error' | 'info' | 'default' {
    switch (status) {
        case 'completed': return 'success'
        case 'running': return 'info'
        case 'failed': return 'error'
        case 'pending': return 'warning'
        default: return 'default'
    }
}

function getActivityColor(type: string): string {
    switch (type) {
        case 'agent': return 'text-blue'
        case 'workflow': return 'text-mauve'
        case 'error': return 'text-red'
        case 'task': return 'text-yellow'
        default: return 'text-subtext0'
    }
}

function getActivityIcon(type: string) {
    switch (type) {
        case 'agent': return <Bot size={14} />
        case 'workflow': return <GitBranch size={14} />
        case 'error': return <AlertCircle size={14} />
        case 'task': return <TrendingUp size={14} />
        default: return <Activity size={14} />
    }
}

// Mock data for initial display
const mockWorkflows = [
    { id: '1', name: 'Data Processing Pipeline', status: 'running', currentStep: 3, totalSteps: 5 },
    { id: '2', name: 'AI Model Training', status: 'completed', currentStep: 4, totalSteps: 4 },
    { id: '3', name: 'Report Generation', status: 'pending', currentStep: 0, totalSteps: 3 },
    { id: '4', name: 'API Integration Test', status: 'failed', currentStep: 2, totalSteps: 6 },
    { id: '5', name: 'Backup Workflow', status: 'running', currentStep: 1, totalSteps: 2 },
]

const mockActivity = [
    { id: '1', timestamp: Date.now() - 60000, type: 'agent', message: 'Agent "analyzer" connected' },
    { id: '2', timestamp: Date.now() - 120000, type: 'workflow', message: 'Started "Data Processing Pipeline"' },
    { id: '3', timestamp: Date.now() - 180000, type: 'info', message: 'System health check passed' },
    { id: '4', timestamp: Date.now() - 240000, type: 'task', message: 'Task queue cleared' },
    { id: '5', timestamp: Date.now() - 300000, type: 'error', message: 'Rate limit exceeded for API calls' },
]
