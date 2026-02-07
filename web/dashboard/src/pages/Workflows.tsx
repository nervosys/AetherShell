import { useState, useEffect } from 'react'
import { GitBranch, Plus, Play, Pause, StopCircle, Eye, ChevronRight, CheckCircle2, XCircle, Clock, Loader2 } from 'lucide-react'
import clsx from 'clsx'
import { Card, Button, Badge, ProgressBar } from '../components/ui'
import { useWorkflows, useDashboardStore, type Workflow, type WorkflowStep } from '../store'

export default function Workflows() {
    const workflows = useWorkflows()
    const connect = useDashboardStore(state => state.connect)
    const [selectedWorkflowId, setSelectedWorkflowId] = useState<string | null>(null)

    // Ensure WebSocket connection
    useEffect(() => { connect() }, [connect])

    const displayWorkflows = workflows
    const selectedWorkflow = displayWorkflows.find(w => w.id === selectedWorkflowId)

    return (
        <div className="space-y-6 animate-fade-in">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-2xl font-bold text-text">Workflows</h1>
                    <p className="text-subtext0 mt-1">Orchestrate and monitor workflow execution</p>
                </div>
                <Button>
                    <Plus size={18} />
                    Create Workflow
                </Button>
            </div>

            {/* Stats */}
            <div className="grid grid-cols-4 gap-4">
                <StatBox label="Running" value={displayWorkflows.filter(w => w.status === 'running').length} color="blue" />
                <StatBox label="Completed" value={displayWorkflows.filter(w => w.status === 'completed').length} color="green" />
                <StatBox label="Failed" value={displayWorkflows.filter(w => w.status === 'failed').length} color="red" />
                <StatBox label="Pending" value={displayWorkflows.filter(w => w.status === 'pending').length} color="yellow" />
            </div>

            {/* Content */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                {/* Workflow List */}
                <div className="lg:col-span-2 space-y-4">
                    {displayWorkflows.map((workflow) => (
                        <WorkflowCard
                            key={workflow.id}
                            workflow={workflow}
                            isSelected={workflow.id === selectedWorkflowId}
                            onSelect={() => setSelectedWorkflowId(workflow.id === selectedWorkflowId ? null : workflow.id)}
                        />
                    ))}
                </div>

                {/* Workflow Details */}
                <div>
                    {selectedWorkflow ? (
                        <WorkflowDetails workflow={selectedWorkflow} />
                    ) : (
                        <Card>
                            <div className="text-center py-12 text-subtext0">
                                <GitBranch size={48} className="mx-auto mb-4 opacity-50" />
                                <p>Select a workflow to view details</p>
                            </div>
                        </Card>
                    )}
                </div>
            </div>
        </div>
    )
}

interface StatBoxProps {
    label: string
    value: number
    color: 'blue' | 'green' | 'red' | 'yellow'
}

function StatBox({ label, value, color }: StatBoxProps) {
    const colors = {
        blue: 'border-blue text-blue',
        green: 'border-green text-green',
        red: 'border-red text-red',
        yellow: 'border-yellow text-yellow',
    }

    return (
        <div className={clsx('bg-surface0 rounded-lg p-4 border-l-4', colors[color])}>
            <p className="text-2xl font-bold">{value}</p>
            <p className="text-sm text-subtext0">{label}</p>
        </div>
    )
}

interface WorkflowCardProps {
    workflow: Workflow
    isSelected: boolean
    onSelect: () => void
}

function WorkflowCard({ workflow, isSelected, onSelect }: WorkflowCardProps) {
    const cancelWorkflow = useDashboardStore(state => state.cancelWorkflow)

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
                        <StatusIcon status={workflow.status} />
                        <div>
                            <h3 className="font-semibold text-text">{workflow.name}</h3>
                            <p className="text-sm text-subtext0">
                                Step {workflow.currentStep} of {workflow.totalSteps}
                            </p>
                        </div>
                    </div>
                    <Badge variant={getStatusVariant(workflow.status)}>
                        {workflow.status}
                    </Badge>
                </div>

                <div className="mt-4">
                    <ProgressBar
                        value={workflow.currentStep}
                        max={workflow.totalSteps}
                        color={getProgressColor(workflow.status)}
                    />
                </div>

                {workflow.status === 'running' && (
                    <div className="mt-4 flex gap-2" onClick={(e) => e.stopPropagation()}>
                        <Button size="sm" variant="secondary">
                            <Pause size={14} />
                            Pause
                        </Button>
                        <Button size="sm" variant="danger" onClick={() => cancelWorkflow(workflow.id)}>
                            <StopCircle size={14} />
                            Cancel
                        </Button>
                    </div>
                )}

                {workflow.status === 'pending' && (
                    <div className="mt-4" onClick={(e) => e.stopPropagation()}>
                        <Button size="sm">
                            <Play size={14} />
                            Start
                        </Button>
                    </div>
                )}
            </div>
        </Card>
    )
}

interface WorkflowDetailsProps {
    workflow: Workflow
}

function WorkflowDetails({ workflow }: WorkflowDetailsProps) {
    return (
        <Card title="Workflow Steps" padding={false}>
            <div className="p-4 space-y-1">
                {workflow.steps.map((step, index) => (
                    <StepItem
                        key={index}
                        step={step}
                        index={index}
                        isCurrent={index === workflow.currentStep}
                    />
                ))}
            </div>

            {workflow.error && (
                <div className="p-4 border-t border-surface1">
                    <div className="bg-red/10 text-red rounded-lg p-3 text-sm">
                        <p className="font-semibold mb-1">Error</p>
                        <p>{workflow.error}</p>
                    </div>
                </div>
            )}
        </Card>
    )
}

interface StepItemProps {
    step: WorkflowStep
    index: number
    isCurrent: boolean
}

function StepItem({ step, index, isCurrent }: StepItemProps) {
    return (
        <div className={clsx(
            'flex items-center gap-3 p-3 rounded-lg',
            isCurrent && 'bg-mauve/10'
        )}>
            <StepStatusIcon status={step.status} />
            <div className="flex-1">
                <p className={clsx(
                    'font-medium',
                    step.status === 'completed' ? 'text-text' :
                        step.status === 'running' ? 'text-mauve' :
                            step.status === 'failed' ? 'text-red' : 'text-subtext0'
                )}>
                    {index + 1}. {step.name}
                </p>
                {step.error && (
                    <p className="text-xs text-red mt-1">{step.error}</p>
                )}
            </div>
            {isCurrent && step.status === 'running' && (
                <ChevronRight size={18} className="text-mauve animate-pulse" />
            )}
        </div>
    )
}

function StatusIcon({ status }: { status: Workflow['status'] }) {
    const icons = {
        pending: <Clock size={20} className="text-yellow" />,
        running: <Loader2 size={20} className="text-blue animate-spin" />,
        completed: <CheckCircle2 size={20} className="text-green" />,
        failed: <XCircle size={20} className="text-red" />,
        cancelled: <StopCircle size={20} className="text-surface2" />,
    }
    return icons[status]
}

function StepStatusIcon({ status }: { status: WorkflowStep['status'] }) {
    switch (status) {
        case 'completed': return <CheckCircle2 size={16} className="text-green" />
        case 'running': return <Loader2 size={16} className="text-blue animate-spin" />
        case 'failed': return <XCircle size={16} className="text-red" />
        case 'skipped': return <Eye size={16} className="text-surface2" />
        default: return <div className="w-4 h-4 rounded-full border-2 border-surface2" />
    }
}

function getStatusVariant(status: Workflow['status']) {
    switch (status) {
        case 'completed': return 'success' as const
        case 'running': return 'info' as const
        case 'failed': return 'error' as const
        case 'pending': return 'warning' as const
        default: return 'default' as const
    }
}

function getProgressColor(status: Workflow['status']) {
    switch (status) {
        case 'completed': return 'green' as const
        case 'running': return 'blue' as const
        case 'failed': return 'red' as const
        default: return 'yellow' as const
    }
}

// Mock workflows
const mockWorkflows: Workflow[] = [
    {
        id: 'wf-001',
        name: 'Data ETL Pipeline',
        status: 'running',
        currentStep: 2,
        totalSteps: 5,
        startedAt: Date.now() - 300000,
        steps: [
            { name: 'Extract from source', status: 'completed' },
            { name: 'Validate schema', status: 'completed' },
            { name: 'Transform data', status: 'running' },
            { name: 'Load to warehouse', status: 'pending' },
            { name: 'Generate report', status: 'pending' },
        ]
    },
    {
        id: 'wf-002',
        name: 'Model Training',
        status: 'completed',
        currentStep: 4,
        totalSteps: 4,
        startedAt: Date.now() - 3600000,
        completedAt: Date.now() - 1800000,
        steps: [
            { name: 'Prepare dataset', status: 'completed' },
            { name: 'Train model', status: 'completed' },
            { name: 'Evaluate metrics', status: 'completed' },
            { name: 'Deploy to production', status: 'completed' },
        ]
    },
    {
        id: 'wf-003',
        name: 'Security Scan',
        status: 'failed',
        currentStep: 2,
        totalSteps: 4,
        startedAt: Date.now() - 600000,
        error: 'Vulnerability CVE-2024-1234 detected in dependency',
        steps: [
            { name: 'Scan dependencies', status: 'completed' },
            { name: 'Static analysis', status: 'completed' },
            { name: 'Runtime analysis', status: 'failed', error: 'Critical vulnerability found' },
            { name: 'Generate report', status: 'skipped' },
        ]
    },
    {
        id: 'wf-004',
        name: 'Backup & Archive',
        status: 'pending',
        currentStep: 0,
        totalSteps: 3,
        startedAt: Date.now(),
        steps: [
            { name: 'Create snapshot', status: 'pending' },
            { name: 'Compress data', status: 'pending' },
            { name: 'Upload to S3', status: 'pending' },
        ]
    },
]
