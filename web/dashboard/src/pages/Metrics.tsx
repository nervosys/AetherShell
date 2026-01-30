import { useState } from 'react'
import { LineChart, Line, AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer, BarChart, Bar } from 'recharts'
import { Activity, Clock, Zap, AlertTriangle, RefreshCw } from 'lucide-react'
import clsx from 'clsx'
import { Card, Button, Badge } from '../components/ui'
import { useMetrics } from '../store'

// Time range options
const timeRanges = [
    { label: '1h', value: 3600000 },
    { label: '6h', value: 21600000 },
    { label: '24h', value: 86400000 },
    { label: '7d', value: 604800000 },
]

export default function Metrics() {
    useMetrics() // Keep subscription active
    const [selectedRange, setSelectedRange] = useState(timeRanges[2].value)

    return (
        <div className="space-y-6 animate-fade-in">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-2xl font-bold text-text">Metrics</h1>
                    <p className="text-subtext0 mt-1">Monitor system performance and health</p>
                </div>
                <div className="flex items-center gap-4">
                    <div className="flex bg-surface0 rounded-lg p-1">
                        {timeRanges.map((range) => (
                            <button
                                key={range.value}
                                onClick={() => setSelectedRange(range.value)}
                                className={clsx(
                                    'px-3 py-1.5 rounded text-sm transition-colors',
                                    selectedRange === range.value
                                        ? 'bg-mauve text-base'
                                        : 'text-subtext0 hover:text-text'
                                )}
                            >
                                {range.label}
                            </button>
                        ))}
                    </div>
                    <Button variant="secondary">
                        <RefreshCw size={18} />
                        Refresh
                    </Button>
                </div>
            </div>

            {/* Key Metrics */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <MetricCard
                    icon={<Zap size={20} />}
                    label="Requests/min"
                    value="1,247"
                    change={12.5}
                    color="blue"
                    sparkline={generateSparkline(20)}
                />
                <MetricCard
                    icon={<Clock size={20} />}
                    label="Avg Latency"
                    value="89ms"
                    change={-8.2}
                    color="green"
                    sparkline={generateSparkline(20)}
                />
                <MetricCard
                    icon={<Activity size={20} />}
                    label="Throughput"
                    value="2.4 GB/s"
                    change={5.1}
                    color="mauve"
                    sparkline={generateSparkline(20)}
                />
                <MetricCard
                    icon={<AlertTriangle size={20} />}
                    label="Error Rate"
                    value="0.12%"
                    change={-15.3}
                    color="yellow"
                    sparkline={generateSparkline(20)}
                />
            </div>

            {/* Charts Grid */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                {/* Request Volume */}
                <Card title="Request Volume">
                    <div className="h-64">
                        <ResponsiveContainer width="100%" height="100%">
                            <AreaChart data={requestVolumeData}>
                                <defs>
                                    <linearGradient id="requestGrad" x1="0" y1="0" x2="0" y2="1">
                                        <stop offset="5%" stopColor="#89b4fa" stopOpacity={0.3} />
                                        <stop offset="95%" stopColor="#89b4fa" stopOpacity={0} />
                                    </linearGradient>
                                </defs>
                                <XAxis dataKey="time" stroke="#6c7086" fontSize={12} tickLine={false} />
                                <YAxis stroke="#6c7086" fontSize={12} tickLine={false} axisLine={false} />
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
                                    dataKey="value"
                                    stroke="#89b4fa"
                                    strokeWidth={2}
                                    fill="url(#requestGrad)"
                                />
                            </AreaChart>
                        </ResponsiveContainer>
                    </div>
                </Card>

                {/* Latency Distribution */}
                <Card title="Latency Distribution">
                    <div className="h-64">
                        <ResponsiveContainer width="100%" height="100%">
                            <BarChart data={latencyDistribution}>
                                <XAxis dataKey="bucket" stroke="#6c7086" fontSize={12} tickLine={false} />
                                <YAxis stroke="#6c7086" fontSize={12} tickLine={false} axisLine={false} />
                                <Tooltip
                                    contentStyle={{
                                        backgroundColor: '#313244',
                                        border: '1px solid #45475a',
                                        borderRadius: '8px',
                                        color: '#cdd6f4',
                                    }}
                                />
                                <Bar dataKey="count" fill="#cba6f7" radius={[4, 4, 0, 0]} />
                            </BarChart>
                        </ResponsiveContainer>
                    </div>
                </Card>

                {/* Error Rates */}
                <Card title="Error Rates by Type">
                    <div className="h-64">
                        <ResponsiveContainer width="100%" height="100%">
                            <LineChart data={errorRatesData}>
                                <XAxis dataKey="time" stroke="#6c7086" fontSize={12} tickLine={false} />
                                <YAxis stroke="#6c7086" fontSize={12} tickLine={false} axisLine={false} />
                                <Tooltip
                                    contentStyle={{
                                        backgroundColor: '#313244',
                                        border: '1px solid #45475a',
                                        borderRadius: '8px',
                                        color: '#cdd6f4',
                                    }}
                                />
                                <Line type="monotone" dataKey="timeout" stroke="#f9e2af" strokeWidth={2} dot={false} name="Timeout" />
                                <Line type="monotone" dataKey="validation" stroke="#fab387" strokeWidth={2} dot={false} name="Validation" />
                                <Line type="monotone" dataKey="server" stroke="#f38ba8" strokeWidth={2} dot={false} name="Server" />
                            </LineChart>
                        </ResponsiveContainer>
                    </div>
                </Card>

                {/* Resource Usage */}
                <Card title="Resource Usage">
                    <div className="h-64">
                        <ResponsiveContainer width="100%" height="100%">
                            <LineChart data={resourceData}>
                                <XAxis dataKey="time" stroke="#6c7086" fontSize={12} tickLine={false} />
                                <YAxis stroke="#6c7086" fontSize={12} tickLine={false} axisLine={false} domain={[0, 100]} />
                                <Tooltip
                                    contentStyle={{
                                        backgroundColor: '#313244',
                                        border: '1px solid #45475a',
                                        borderRadius: '8px',
                                        color: '#cdd6f4',
                                    }}
                                />
                                <Line type="monotone" dataKey="cpu" stroke="#a6e3a1" strokeWidth={2} dot={false} name="CPU %" />
                                <Line type="monotone" dataKey="memory" stroke="#89b4fa" strokeWidth={2} dot={false} name="Memory %" />
                            </LineChart>
                        </ResponsiveContainer>
                    </div>
                </Card>
            </div>

            {/* Health Checks */}
            <Card title="Service Health">
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                    <HealthCheck name="API Server" status="healthy" latency={12} />
                    <HealthCheck name="Database" status="healthy" latency={8} />
                    <HealthCheck name="Cache" status="degraded" latency={45} />
                    <HealthCheck name="Message Queue" status="healthy" latency={5} />
                </div>
            </Card>
        </div>
    )
}

interface MetricCardProps {
    icon: React.ReactNode
    label: string
    value: string
    change: number
    color: 'blue' | 'green' | 'yellow' | 'mauve'
    sparkline: { value: number }[]
}

function MetricCard({ icon, label, value, change, color, sparkline }: MetricCardProps) {
    const colors = {
        blue: 'text-blue',
        green: 'text-green',
        yellow: 'text-yellow',
        mauve: 'text-mauve',
    }

    const strokeColors = {
        blue: '#89b4fa',
        green: '#a6e3a1',
        yellow: '#f9e2af',
        mauve: '#cba6f7',
    }

    return (
        <Card>
            <div className="flex items-start justify-between">
                <div>
                    <div className={clsx('mb-2', colors[color])}>{icon}</div>
                    <p className="text-sm text-subtext0">{label}</p>
                    <p className="text-2xl font-bold text-text">{value}</p>
                    <p className={clsx(
                        'text-sm mt-1',
                        change >= 0 ? 'text-green' : 'text-red'
                    )}>
                        {change >= 0 ? '+' : ''}{change}%
                    </p>
                </div>
                <div className="w-20 h-12">
                    <ResponsiveContainer width="100%" height="100%">
                        <LineChart data={sparkline}>
                            <Line
                                type="monotone"
                                dataKey="value"
                                stroke={strokeColors[color]}
                                strokeWidth={2}
                                dot={false}
                            />
                        </LineChart>
                    </ResponsiveContainer>
                </div>
            </div>
        </Card>
    )
}

interface HealthCheckProps {
    name: string
    status: 'healthy' | 'degraded' | 'unhealthy'
    latency: number
}

function HealthCheck({ name, status, latency }: HealthCheckProps) {
    const statusConfig = {
        healthy: { color: 'bg-green', label: 'Healthy', variant: 'success' as const },
        degraded: { color: 'bg-yellow', label: 'Degraded', variant: 'warning' as const },
        unhealthy: { color: 'bg-red', label: 'Unhealthy', variant: 'error' as const },
    }

    const config = statusConfig[status]

    return (
        <div className="flex items-center justify-between p-3 bg-surface1/50 rounded-lg">
            <div className="flex items-center gap-3">
                <div className={clsx('w-3 h-3 rounded-full', config.color)} />
                <span className="font-medium text-text">{name}</span>
            </div>
            <div className="flex items-center gap-3">
                <span className="text-sm text-subtext0">{latency}ms</span>
                <Badge variant={config.variant}>{config.label}</Badge>
            </div>
        </div>
    )
}

// Helper to generate sparkline data
function generateSparkline(points: number) {
    return Array.from({ length: points }, () => ({
        value: Math.random() * 100 + 50,
    }))
}

// Mock data
const requestVolumeData = Array.from({ length: 24 }, (_, i) => ({
    time: `${String(i).padStart(2, '0')}:00`,
    value: Math.floor(Math.random() * 500) + 500,
}))

const latencyDistribution = [
    { bucket: '0-50ms', count: 1250 },
    { bucket: '50-100ms', count: 890 },
    { bucket: '100-200ms', count: 450 },
    { bucket: '200-500ms', count: 180 },
    { bucket: '500ms+', count: 45 },
]

const errorRatesData = Array.from({ length: 24 }, (_, i) => ({
    time: `${String(i).padStart(2, '0')}:00`,
    timeout: Math.random() * 0.5,
    validation: Math.random() * 0.3,
    server: Math.random() * 0.1,
}))

const resourceData = Array.from({ length: 24 }, (_, i) => ({
    time: `${String(i).padStart(2, '0')}:00`,
    cpu: Math.random() * 40 + 30,
    memory: Math.random() * 30 + 50,
}))
