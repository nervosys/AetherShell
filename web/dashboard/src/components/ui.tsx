import { ReactNode } from 'react'
import clsx from 'clsx'

interface CardProps {
    title?: string
    children: ReactNode
    className?: string
    padding?: boolean
}

export function Card({ title, children, className, padding = true }: CardProps) {
    return (
        <div className={clsx('bg-surface0 rounded-xl border border-surface1', className)}>
            {title && (
                <div className="px-4 py-3 border-b border-surface1">
                    <h3 className="font-semibold text-text">{title}</h3>
                </div>
            )}
            <div className={padding ? 'p-4' : ''}>
                {children}
            </div>
        </div>
    )
}

interface StatCardProps {
    label: string
    value: string | number
    change?: { value: number; label: string }
    icon?: ReactNode
    color?: 'blue' | 'green' | 'yellow' | 'red' | 'mauve'
}

export function StatCard({ label, value, change, icon, color = 'blue' }: StatCardProps) {
    const colorClasses = {
        blue: 'text-blue bg-blue/10',
        green: 'text-green bg-green/10',
        yellow: 'text-yellow bg-yellow/10',
        red: 'text-red bg-red/10',
        mauve: 'text-mauve bg-mauve/10',
    }

    return (
        <Card className="relative overflow-hidden">
            <div className="flex items-start justify-between">
                <div>
                    <p className="text-sm text-subtext0">{label}</p>
                    <p className="text-2xl font-bold text-text mt-1">{value}</p>
                    {change && (
                        <p className={clsx(
                            'text-sm mt-2',
                            change.value >= 0 ? 'text-green' : 'text-red'
                        )}>
                            {change.value >= 0 ? '+' : ''}{change.value}% {change.label}
                        </p>
                    )}
                </div>
                {icon && (
                    <div className={clsx('p-3 rounded-lg', colorClasses[color])}>
                        {icon}
                    </div>
                )}
            </div>
        </Card>
    )
}

interface BadgeProps {
    children: ReactNode
    variant?: 'default' | 'success' | 'warning' | 'error' | 'info'
}

export function Badge({ children, variant = 'default' }: BadgeProps) {
    const variants = {
        default: 'bg-surface1 text-text',
        success: 'bg-green/20 text-green',
        warning: 'bg-yellow/20 text-yellow',
        error: 'bg-red/20 text-red',
        info: 'bg-blue/20 text-blue',
    }

    return (
        <span className={clsx(
            'inline-flex items-center px-2 py-0.5 rounded text-xs font-medium',
            variants[variant]
        )}>
            {children}
        </span>
    )
}

interface ButtonProps {
    children: ReactNode
    onClick?: () => void
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
    size?: 'sm' | 'md' | 'lg'
    disabled?: boolean
    className?: string
}

export function Button({
    children,
    onClick,
    variant = 'primary',
    size = 'md',
    disabled,
    className
}: ButtonProps) {
    const variants = {
        primary: 'bg-mauve text-base hover:bg-mauve/90',
        secondary: 'bg-surface1 text-text hover:bg-surface2',
        ghost: 'bg-transparent text-subtext0 hover:text-text hover:bg-surface0',
        danger: 'bg-red text-base hover:bg-red/90',
    }

    const sizes = {
        sm: 'px-2.5 py-1.5 text-sm',
        md: 'px-4 py-2',
        lg: 'px-6 py-3 text-lg',
    }

    return (
        <button
            onClick={onClick}
            disabled={disabled}
            className={clsx(
                'rounded-lg font-medium transition-colors inline-flex items-center gap-2',
                variants[variant],
                sizes[size],
                disabled && 'opacity-50 cursor-not-allowed',
                className
            )}
        >
            {children}
        </button>
    )
}

interface ProgressBarProps {
    value: number
    max?: number
    label?: string
    color?: 'blue' | 'green' | 'yellow' | 'red' | 'mauve'
}

export function ProgressBar({ value, max = 100, label, color = 'blue' }: ProgressBarProps) {
    const percentage = Math.min(100, (value / max) * 100)

    const colors = {
        blue: 'bg-blue',
        green: 'bg-green',
        yellow: 'bg-yellow',
        red: 'bg-red',
        mauve: 'bg-mauve',
    }

    return (
        <div>
            {label && (
                <div className="flex justify-between text-sm mb-1">
                    <span className="text-subtext0">{label}</span>
                    <span className="text-text">{value}/{max}</span>
                </div>
            )}
            <div className="h-2 bg-surface1 rounded-full overflow-hidden">
                <div
                    className={clsx('h-full rounded-full transition-all', colors[color])}
                    style={{ width: `${percentage}%` }}
                />
            </div>
        </div>
    )
}
