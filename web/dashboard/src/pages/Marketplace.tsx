import { useState, useEffect, useCallback } from 'react'
import { Search, Download, Star, GitFork, Tag, User, Clock, Filter, ChevronDown, Check, Loader2, Upload } from 'lucide-react'
import clsx from 'clsx'
import { Card, Button, Badge } from '../components/ui'
import { useDashboardStore, type MarketplaceAgent } from '../store/dashboard'

export default function Marketplace() {
    const [searchQuery, setSearchQuery] = useState('')
    const [selectedCategory, setSelectedCategory] = useState('all')
    const [sortBy, setSortBy] = useState('downloads')
    const [agents, setAgents] = useState<MarketplaceAgent[]>(mockMarketplaceAgents)
    const [loading, setLoading] = useState(false)
    const [showPublish, setShowPublish] = useState(false)
    const { searchMarketplace, publishAgent } = useDashboardStore()

    // Fetch from API on mount and when search/category changes
    const fetchAgents = useCallback(async () => {
        try {
            setLoading(true)
            const results = await searchMarketplace(searchQuery, selectedCategory)
            if (results.length > 0) {
                setAgents(results)
            } else if (!searchQuery && selectedCategory === 'all') {
                setAgents(mockMarketplaceAgents) // fallback to mock
            } else {
                setAgents([])
            }
        } catch {
            // API not available, keep current data
        } finally {
            setLoading(false)
        }
    }, [searchQuery, selectedCategory, searchMarketplace])

    useEffect(() => {
        fetchAgents()
    }, [fetchAgents])

    const filteredAgents = agents
        .sort((a, b) => {
            switch (sortBy) {
                case 'downloads': return b.downloads - a.downloads
                case 'stars': return b.stars - a.stars
                case 'updated': return b.updatedAt - a.updatedAt
                default: return 0
            }
        })

    return (
        <div className="space-y-6 animate-fade-in">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-2xl font-bold text-text">Marketplace</h1>
                    <p className="text-subtext0 mt-1">Discover and install community agents</p>
                </div>
                <Button onClick={() => setShowPublish(true)}>
                    <Upload size={16} />
                    Publish Agent
                </Button>
            </div>

            {/* Publish Dialog */}
            {showPublish && (
                <PublishDialog
                    onClose={() => setShowPublish(false)}
                    onPublish={async (def) => {
                        await publishAgent(def)
                        setShowPublish(false)
                        fetchAgents()
                    }}
                />
            )}

            {/* Search & Filters */}
            <div className="flex flex-col md:flex-row gap-4">
                <div className="flex-1 relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-subtext0" size={18} />
                    <input
                        type="text"
                        placeholder="Search agents, tools, workflows..."
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        className="w-full pl-10 pr-4 py-2.5 bg-surface0 border border-surface1 rounded-lg text-text placeholder:text-subtext0 focus:outline-none focus:border-mauve"
                    />
                </div>

                <div className="flex gap-3">
                    <div className="relative">
                        <Filter size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-subtext0" />
                        <select
                            value={selectedCategory}
                            onChange={(e) => setSelectedCategory(e.target.value)}
                            className="appearance-none pl-9 pr-10 py-2.5 bg-surface0 border border-surface1 rounded-lg text-text focus:outline-none focus:border-mauve"
                        >
                            <option value="all">All Categories</option>
                            <option value="data">Data Processing</option>
                            <option value="code">Code Analysis</option>
                            <option value="ai">AI/ML</option>
                            <option value="devops">DevOps</option>
                            <option value="security">Security</option>
                        </select>
                        <ChevronDown size={16} className="absolute right-3 top-1/2 -translate-y-1/2 text-subtext0 pointer-events-none" />
                    </div>

                    <div className="relative">
                        <select
                            value={sortBy}
                            onChange={(e) => setSortBy(e.target.value)}
                            className="appearance-none pl-4 pr-10 py-2.5 bg-surface0 border border-surface1 rounded-lg text-text focus:outline-none focus:border-mauve"
                        >
                            <option value="downloads">Most Downloads</option>
                            <option value="stars">Most Stars</option>
                            <option value="updated">Recently Updated</option>
                        </select>
                        <ChevronDown size={16} className="absolute right-3 top-1/2 -translate-y-1/2 text-subtext0 pointer-events-none" />
                    </div>
                </div>
            </div>

            {/* Categories */}
            <div className="flex gap-2 flex-wrap">
                {categories.map((cat) => (
                    <button
                        key={cat.id}
                        onClick={() => setSelectedCategory(cat.id)}
                        className={clsx(
                            'px-4 py-2 rounded-lg text-sm transition-colors',
                            selectedCategory === cat.id
                                ? 'bg-mauve text-base'
                                : 'bg-surface0 text-subtext0 hover:text-text hover:bg-surface1'
                        )}
                    >
                        {cat.icon} {cat.label}
                    </button>
                ))}
            </div>

            {/* Agent Grid */}
            {loading ? (
                <Card>
                    <div className="text-center py-12 text-subtext0">
                        <Loader2 size={48} className="mx-auto mb-4 animate-spin opacity-50" />
                        <p>Searching marketplace...</p>
                    </div>
                </Card>
            ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    {filteredAgents.map((agent) => (
                        <AgentCard key={agent.id} agent={agent} onRefresh={fetchAgents} />
                    ))}
                </div>
            )}

            {filteredAgents.length === 0 && (
                <Card>
                    <div className="text-center py-12 text-subtext0">
                        <Search size={48} className="mx-auto mb-4 opacity-50" />
                        <p>No agents found matching your criteria</p>
                    </div>
                </Card>
            )}
        </div>
    )
}

interface AgentCardProps {
    agent: MarketplaceAgent
    onRefresh: () => void
}

function AgentCard({ agent, onRefresh }: AgentCardProps) {
    const [installing, setInstalling] = useState(false)
    const [installed, setInstalled] = useState(agent.installed || false)
    const [error, setError] = useState<string | null>(null)
    const { installMarketplaceAgent } = useDashboardStore()

    const handleInstall = async () => {
        try {
            setInstalling(true)
            setError(null)
            await installMarketplaceAgent(agent.id, agent.version)
            setInstalled(true)
            onRefresh()
        } catch (e) {
            setError(e instanceof Error ? e.message : 'Install failed')
        } finally {
            setInstalling(false)
        }
    }
    const formatNumber = (n: number) => {
        if (n >= 1000) return `${(n / 1000).toFixed(1)}k`
        return n.toString()
    }

    const timeAgo = (timestamp: number) => {
        const seconds = Math.floor((Date.now() - timestamp) / 1000)
        if (seconds < 60) return 'just now'
        const minutes = Math.floor(seconds / 60)
        if (minutes < 60) return `${minutes}m ago`
        const hours = Math.floor(minutes / 60)
        if (hours < 24) return `${hours}h ago`
        const days = Math.floor(hours / 24)
        return `${days}d ago`
    }

    return (
        <Card className="hover:border-mauve/50 transition-colors" padding={false}>
            <div className="p-4">
                <div className="flex items-start justify-between">
                    <div className="flex-1">
                        <div className="flex items-center gap-2">
                            <h3 className="font-semibold text-text">{agent.name}</h3>
                            {agent.verified && (
                                <Badge variant="success">✓ Verified</Badge>
                            )}
                        </div>
                        <div className="flex items-center gap-2 text-sm text-subtext0 mt-1">
                            <User size={14} />
                            <span>{agent.author}</span>
                            <span>•</span>
                            <Tag size={14} />
                            <span>v{agent.version}</span>
                        </div>
                    </div>
                </div>

                <p className="text-sm text-subtext0 mt-3 line-clamp-2">{agent.description}</p>

                <div className="flex flex-wrap gap-2 mt-3">
                    {agent.tags.slice(0, 3).map((tag) => (
                        <Badge key={tag}>{tag}</Badge>
                    ))}
                </div>
            </div>

            <div className="px-4 py-3 bg-surface1/30 border-t border-surface1 flex items-center justify-between">
                <div className="flex items-center gap-4 text-sm text-subtext0">
                    <span className="flex items-center gap-1">
                        <Download size={14} />
                        {formatNumber(agent.downloads)}
                    </span>
                    <span className="flex items-center gap-1">
                        <Star size={14} />
                        {formatNumber(agent.stars)}
                    </span>
                    <span className="flex items-center gap-1">
                        <GitFork size={14} />
                        {formatNumber(agent.forks)}
                    </span>
                </div>
                <div className="flex items-center gap-1 text-xs text-subtext0">
                    <Clock size={12} />
                    {timeAgo(agent.updatedAt)}
                </div>
            </div>

            <div className="p-3 border-t border-surface1">
                {error && (
                    <p className="text-xs text-red mb-2">{error}</p>
                )}
                <Button
                    className="w-full"
                    size="sm"
                    onClick={handleInstall}
                    disabled={installing || installed}
                >
                    {installing ? (
                        <><Loader2 size={14} className="animate-spin" /> Installing...</>
                    ) : installed ? (
                        <><Check size={14} /> Installed</>
                    ) : (
                        <><Download size={14} /> Install</>
                    )}
                </Button>
            </div>
        </Card>
    )
}

// Publish Dialog
function PublishDialog({ onClose, onPublish }: {
    onClose: () => void
    onPublish: (def: { name: string; description: string; systemPrompt: string; tools: string[]; model?: string }) => Promise<void>
}) {
    const [name, setName] = useState('')
    const [description, setDescription] = useState('')
    const [systemPrompt, setSystemPrompt] = useState('')
    const [tools, setTools] = useState('')
    const [model, setModel] = useState('')
    const [publishing, setPublishing] = useState(false)
    const [error, setError] = useState<string | null>(null)

    const handleSubmit = async () => {
        if (!name || !description || !systemPrompt) {
            setError('Name, description, and system prompt are required')
            return
        }
        try {
            setPublishing(true)
            setError(null)
            await onPublish({
                name,
                description,
                systemPrompt,
                tools: tools.split(',').map(t => t.trim()).filter(Boolean),
                model: model || undefined,
            })
        } catch (e) {
            setError(e instanceof Error ? e.message : 'Publish failed')
            setPublishing(false)
        }
    }

    return (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={onClose}>
            <div className="w-full max-w-lg bg-surface0 rounded-xl border border-surface1" onClick={(e) => e.stopPropagation()}>
                <div className="p-6 space-y-4">
                    <h2 className="text-xl font-bold text-text">Publish Agent</h2>
                    {error && <p className="text-sm text-red">{error}</p>}
                    <div>
                        <label className="block text-sm text-subtext0 mb-1">Agent Name</label>
                        <input value={name} onChange={e => setName(e.target.value)}
                            className="w-full px-3 py-2 bg-surface0 border border-surface1 rounded-lg text-text focus:outline-none focus:border-mauve"
                            placeholder="my-agent" />
                    </div>
                    <div>
                        <label className="block text-sm text-subtext0 mb-1">Description</label>
                        <input value={description} onChange={e => setDescription(e.target.value)}
                            className="w-full px-3 py-2 bg-surface0 border border-surface1 rounded-lg text-text focus:outline-none focus:border-mauve"
                            placeholder="What does your agent do?" />
                    </div>
                    <div>
                        <label className="block text-sm text-subtext0 mb-1">System Prompt</label>
                        <textarea value={systemPrompt} onChange={e => setSystemPrompt(e.target.value)} rows={3}
                            className="w-full px-3 py-2 bg-surface0 border border-surface1 rounded-lg text-text focus:outline-none focus:border-mauve resize-none"
                            placeholder="You are a helpful agent that..." />
                    </div>
                    <div>
                        <label className="block text-sm text-subtext0 mb-1">Tools (comma-separated)</label>
                        <input value={tools} onChange={e => setTools(e.target.value)}
                            className="w-full px-3 py-2 bg-surface0 border border-surface1 rounded-lg text-text focus:outline-none focus:border-mauve"
                            placeholder="ls, cat, git, http_get" />
                    </div>
                    <div>
                        <label className="block text-sm text-subtext0 mb-1">Model (optional)</label>
                        <input value={model} onChange={e => setModel(e.target.value)}
                            className="w-full px-3 py-2 bg-surface0 border border-surface1 rounded-lg text-text focus:outline-none focus:border-mauve"
                            placeholder="openai:gpt-4o-mini" />
                    </div>
                    <div className="flex gap-3 justify-end pt-2">
                        <Button onClick={onClose} size="sm">Cancel</Button>
                        <Button onClick={handleSubmit} disabled={publishing} size="sm">
                            {publishing ? <><Loader2 size={14} className="animate-spin" /> Publishing...</> : 'Publish'}
                        </Button>
                    </div>
                </div>
            </div>
        </div>
    )
}

// Categories
const categories = [
    { id: 'all', label: 'All', icon: '📦' },
    { id: 'data', label: 'Data', icon: '📊' },
    { id: 'code', label: 'Code', icon: '💻' },
    { id: 'ai', label: 'AI/ML', icon: '🤖' },
    { id: 'devops', label: 'DevOps', icon: '🔧' },
    { id: 'security', label: 'Security', icon: '🔒' },
]

// Mock marketplace data
const mockMarketplaceAgents: MarketplaceAgent[] = [
    {
        id: '1',
        name: 'code-reviewer',
        description: 'Automated code review agent with support for multiple languages. Finds bugs, suggests improvements, and enforces coding standards.',
        author: 'nervosys',
        version: '2.1.0',
        downloads: 15234,
        stars: 892,
        forks: 156,
        tags: ['code', 'review', 'quality'],
        updatedAt: Date.now() - 86400000,
        verified: true,
    },
    {
        id: '2',
        name: 'data-transformer',
        description: 'Powerful ETL agent for data transformation pipelines. Supports JSON, CSV, Parquet, and more.',
        author: 'dataforge',
        version: '1.5.2',
        downloads: 8921,
        stars: 445,
        forks: 89,
        tags: ['data', 'etl', 'transform'],
        updatedAt: Date.now() - 172800000,
        verified: true,
    },
    {
        id: '3',
        name: 'security-scanner',
        description: 'Comprehensive security scanning agent. Detects vulnerabilities, secrets in code, and OWASP Top 10 issues.',
        author: 'securitylab',
        version: '3.0.1',
        downloads: 12456,
        stars: 678,
        forks: 234,
        tags: ['security', 'scan', 'vulnerability'],
        updatedAt: Date.now() - 259200000,
        verified: true,
    },
    {
        id: '4',
        name: 'ml-trainer',
        description: 'Machine learning model training orchestrator. Supports PyTorch, TensorFlow, and scikit-learn workflows.',
        author: 'aiworks',
        version: '1.2.0',
        downloads: 5678,
        stars: 312,
        forks: 67,
        tags: ['ai', 'ml', 'training'],
        updatedAt: Date.now() - 345600000,
        verified: false,
    },
    {
        id: '5',
        name: 'k8s-deployer',
        description: 'Kubernetes deployment automation agent. Handles rolling updates, canary deployments, and rollbacks.',
        author: 'cloudops',
        version: '2.0.0',
        downloads: 9876,
        stars: 567,
        forks: 123,
        tags: ['devops', 'kubernetes', 'deploy'],
        updatedAt: Date.now() - 432000000,
        verified: true,
    },
    {
        id: '6',
        name: 'doc-generator',
        description: 'Automatic documentation generator from code. Supports JSDoc, TypeDoc, and Rustdoc formats.',
        author: 'doctools',
        version: '1.0.5',
        downloads: 4321,
        stars: 234,
        forks: 45,
        tags: ['code', 'documentation', 'generate'],
        updatedAt: Date.now() - 518400000,
        verified: false,
    },
]
