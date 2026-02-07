import { useState, useEffect } from 'react'
import { Bot, Plus, Search, Filter, MoreVertical, Zap, Clock, AlertTriangle } from 'lucide-react'
import clsx from 'clsx'
import { Card, Button, Badge } from '../components/ui'
import { useAgents, useDashboardStore, type Agent } from '../store'

export default function Agents() {
    const agents = useAgents()
    const connect = useDashboardStore(state => state.connect)
    const selectAgent = useDashboardStore(state => state.selectAgent)
    const selectedAgentId = useDashboardStore(state => state.selectedAgentId)
    const [searchQuery, setSearchQuery] = useState('')
    const [statusFilter, setStatusFilter] = useState<string>('all')

    // Ensure WebSocket connection
    useEffect(() => { connect() }, [connect])

    // Use real agents from WebSocket, or an empty list
    const displayAgents = agents

    const filteredAgents = displayAgents.filter(agent => {
        const matchesSearch = agent.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
            agent.capabilities.some(c => c.toLowerCase().includes(searchQuery.toLowerCase()))
        const matchesStatus = statusFilter === 'all' || agent.status === statusFilter
        return matchesSearch && matchesStatus
    })

    const selectedAgent = displayAgents.find(a => a.id === selectedAgentId)

    return (
        <div className="space-y-6 animate-fade-in">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-2xl font-bold text-text">Agents</h1>
                    <p className="text-subtext0 mt-1">Manage and monitor your AI agents</p>
                </div>
                <Button>
                    <Plus size={18} />
                    Create Agent
                </Button>
            </div>

            {/* Filters */}
            <div className="flex gap-4">
                <div className="flex-1 relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-subtext0" size={18} />
                    <input
                        type="text"
                        placeholder="Search agents..."
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        className="w-full pl-10 pr-4 py-2 bg-surface0 border border-surface1 rounded-lg text-text placeholder:text-subtext0 focus:outline-none focus:border-mauve"
                    />
                </div>
                <div className="flex items-center gap-2">
                    <Filter size={18} className="text-subtext0" />
                    <select
                        value={statusFilter}
                        onChange={(e) => setStatusFilter(e.target.value)}
                        className="bg-surface0 border border-surface1 rounded-lg px-3 py-2 text-text focus:outline-none focus:border-mauve"
                    >
                        <option value="all">All Status</option>
                        <option value="online">Online</option>
                        <option value="busy">Busy</option>
                        <option value="offline">Offline</option>
                    </select>
                </div>
            </div>

            {/* Content */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                {/* Agent List */}
                <div className="lg:col-span-2 space-y-4">
                    {filteredAgents.map((agent) => (
                        <AgentCard
                            key={agent.id}
                            agent={agent}
                            isSelected={agent.id === selectedAgentId}
                            onSelect={() => selectAgent(agent.id === selectedAgentId ? null : agent.id)}
                        />
                    ))}
                    {filteredAgents.length === 0 && (
                        <Card>
                            <div className="text-center py-8 text-subtext0">
                                <Bot size={48} className="mx-auto mb-4 opacity-50" />
                                <p>No agents found matching your criteria</p>
                            </div>
                        </Card>
                    )}
                </div>

                {/* Agent Details */}
                <div>
                    {selectedAgent ? (
                        <AgentDetails agent={selectedAgent} />
                    ) : (
                        <Card>
                            <div className="text-center py-12 text-subtext0">
                                <Bot size={48} className="mx-auto mb-4 opacity-50" />
                                <p>Select an agent to view details</p>
                            </div>
                        </Card>
                    )}
                </div>
            </div>
        </div>
    )
}

interface AgentCardProps {
    agent: Agent
    isSelected: boolean
    onSelect: () => void
}

function AgentCard({ agent, isSelected, onSelect }: AgentCardProps) {
    const statusColors = {
        online: 'bg-green',
        busy: 'bg-yellow',
        offline: 'bg-surface2',
    }

    return (
        <Card
            className={clsx(
                'cursor-pointer transition-all',
                isSelected && 'ring-2 ring-mauve'
            )}
            padding={false}
        >
            <div className="p-4" onClick={onSelect}>
                <div className="flex items-start justify-between">
                    <div className="flex items-center gap-3">
                        <div className="w-12 h-12 rounded-lg bg-gradient-to-br from-blue to-mauve flex items-center justify-center">
                            <Bot size={24} className="text-base" />
                        </div>
                        <div>
                            <div className="flex items-center gap-2">
                                <h3 className="font-semibold text-text">{agent.name}</h3>
                                <div className={clsx('w-2 h-2 rounded-full', statusColors[agent.status])} />
                            </div>
                            <p className="text-sm text-subtext0">{agent.id}</p>
                        </div>
                    </div>
                    <button className="p-2 hover:bg-surface1 rounded-lg">
                        <MoreVertical size={18} className="text-subtext0" />
                    </button>
                </div>

                <div className="mt-4 flex flex-wrap gap-2">
                    {agent.capabilities.slice(0, 4).map((cap) => (
                        <Badge key={cap}>{cap}</Badge>
                    ))}
                    {agent.capabilities.length > 4 && (
                        <Badge>+{agent.capabilities.length - 4}</Badge>
                    )}
                </div>

                <div className="mt-4 grid grid-cols-3 gap-4 text-sm">
                    <div>
                        <p className="text-subtext0">Requests</p>
                        <p className="font-semibold text-text">{agent.metrics.requestsHandled}</p>
                    </div>
                    <div>
                        <p className="text-subtext0">Latency</p>
                        <p className="font-semibold text-text">{agent.metrics.averageLatency}ms</p>
                    </div>
                    <div>
                        <p className="text-subtext0">Error Rate</p>
                        <p className={clsx(
                            'font-semibold',
                            agent.metrics.errorRate > 5 ? 'text-red' : 'text-green'
                        )}>
                            {agent.metrics.errorRate}%
                        </p>
                    </div>
                </div>
            </div>
        </Card>
    )
}

interface AgentDetailsProps {
    agent: Agent
}

function AgentDetails({ agent }: AgentDetailsProps) {
    return (
        <Card title="Agent Details" padding={false}>
            <div className="p-4 space-y-6">
                {/* Status */}
                <div className="flex items-center justify-between">
                    <span className="text-subtext0">Status</span>
                    <Badge variant={agent.status === 'online' ? 'success' : agent.status === 'busy' ? 'warning' : 'default'}>
                        {agent.status}
                    </Badge>
                </div>

                {/* Capabilities */}
                <div>
                    <h4 className="text-sm text-subtext0 mb-2">Capabilities</h4>
                    <div className="flex flex-wrap gap-2">
                        {agent.capabilities.map((cap) => (
                            <Badge key={cap}>{cap}</Badge>
                        ))}
                    </div>
                </div>

                {/* Metrics */}
                <div>
                    <h4 className="text-sm text-subtext0 mb-3">Performance</h4>
                    <div className="space-y-3">
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2 text-sm">
                                <Zap size={14} className="text-yellow" />
                                <span>Requests Handled</span>
                            </div>
                            <span className="font-mono text-text">{agent.metrics.requestsHandled}</span>
                        </div>
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2 text-sm">
                                <Clock size={14} className="text-blue" />
                                <span>Avg Latency</span>
                            </div>
                            <span className="font-mono text-text">{agent.metrics.averageLatency}ms</span>
                        </div>
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2 text-sm">
                                <AlertTriangle size={14} className="text-red" />
                                <span>Error Rate</span>
                            </div>
                            <span className="font-mono text-text">{agent.metrics.errorRate}%</span>
                        </div>
                    </div>
                </div>

                {/* Actions */}
                <div className="pt-4 border-t border-surface1 space-y-2">
                    <Button className="w-full" variant="secondary">View Logs</Button>
                    <Button className="w-full" variant="ghost">Configure</Button>
                </div>
            </div>
        </Card>
    )
}
