import { Outlet, NavLink } from 'react-router-dom'
import { useEffect } from 'react'
import {
    LayoutDashboard,
    Bot,
    GitBranch,
    BarChart3,
    Store,
    Terminal as TerminalIcon,
    Wifi,
    WifiOff,
    Loader2
} from 'lucide-react'
import clsx from 'clsx'
import { useDashboardStore, useConnectionStatus } from '../store'

const navItems = [
    { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
    { to: '/agents', icon: Bot, label: 'Agents' },
    { to: '/workflows', icon: GitBranch, label: 'Workflows' },
    { to: '/metrics', icon: BarChart3, label: 'Metrics' },
    { to: '/marketplace', icon: Store, label: 'Marketplace' },
    { to: '/terminal', icon: TerminalIcon, label: 'Terminal' },
]

function ConnectionIndicator() {
    const status = useConnectionStatus()

    const statusConfig: Record<string, { icon: typeof Wifi; color: string; label: string; spin?: boolean }> = {
        connected: { icon: Wifi, color: 'text-green', label: 'Connected' },
        connecting: { icon: Loader2, color: 'text-yellow', label: 'Connecting...', spin: true },
        disconnected: { icon: WifiOff, color: 'text-red', label: 'Disconnected' },
        error: { icon: WifiOff, color: 'text-red', label: 'Error' },
    }

    const config = statusConfig[status]
    const Icon = config.icon

    return (
        <div className={clsx('flex items-center gap-2 px-3 py-1.5 rounded-lg bg-surface0', config.color)}>
            <Icon size={16} className={config.spin ? 'animate-spin' : ''} />
            <span className="text-sm">{config.label}</span>
        </div>
    )
}

export default function Layout() {
    const connect = useDashboardStore(state => state.connect)

    useEffect(() => {
        connect()
    }, [connect])

    return (
        <div className="min-h-screen flex bg-base">
            {/* Sidebar */}
            <aside className="w-64 bg-mantle border-r border-surface0 flex flex-col">
                {/* Logo */}
                <div className="p-4 border-b border-surface0">
                    <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-mauve to-blue flex items-center justify-center">
                            <span className="text-base font-bold text-lg">Æ</span>
                        </div>
                        <div>
                            <h1 className="font-semibold text-text">AetherShell</h1>
                            <span className="text-xs text-subtext0">Dashboard</span>
                        </div>
                    </div>
                </div>

                {/* Navigation */}
                <nav className="flex-1 p-3">
                    <ul className="space-y-1">
                        {navItems.map(({ to, icon: Icon, label }) => (
                            <li key={to}>
                                <NavLink
                                    to={to}
                                    end={to === '/'}
                                    className={({ isActive }) => clsx(
                                        'flex items-center gap-3 px-3 py-2.5 rounded-lg transition-colors',
                                        isActive
                                            ? 'bg-surface0 text-mauve'
                                            : 'text-subtext0 hover:text-text hover:bg-surface0/50'
                                    )}
                                >
                                    <Icon size={20} />
                                    <span>{label}</span>
                                </NavLink>
                            </li>
                        ))}
                    </ul>
                </nav>

                {/* Connection status */}
                <div className="p-3 border-t border-surface0">
                    <ConnectionIndicator />
                </div>
            </aside>

            {/* Main content */}
            <main className="flex-1 overflow-auto">
                <div className="p-6">
                    <Outlet />
                </div>
            </main>
        </div>
    )
}
