import { useState, useRef, useEffect, useCallback } from 'react'
import { Send, Loader2, Trash2, Download, Copy, Check } from 'lucide-react'
import clsx from 'clsx'
import { Card, Button } from '../components/ui'
import { useDashboardStore, useConnectionStatus } from '../store'

interface HistoryEntry {
    id: string
    type: 'input' | 'output' | 'error'
    content: string
    timestamp: number
}

export default function Terminal() {
    const executeCommand = useDashboardStore(state => state.executeCommand)
    const connectionStatus = useConnectionStatus()

    const [input, setInput] = useState('')
    const [history, setHistory] = useState<HistoryEntry[]>([])
    const [isExecuting, setIsExecuting] = useState(false)
    const [commandHistory, setCommandHistory] = useState<string[]>([])
    const [historyIndex, setHistoryIndex] = useState(-1)
    const [copied, setCopied] = useState(false)

    const terminalRef = useRef<HTMLDivElement>(null)
    const inputRef = useRef<HTMLInputElement>(null)

    // Auto-scroll to bottom
    useEffect(() => {
        if (terminalRef.current) {
            terminalRef.current.scrollTop = terminalRef.current.scrollHeight
        }
    }, [history])

    // Focus input on mount
    useEffect(() => {
        inputRef.current?.focus()
    }, [])

    const addToHistory = useCallback((type: HistoryEntry['type'], content: string) => {
        setHistory(prev => [...prev, {
            id: crypto.randomUUID(),
            type,
            content,
            timestamp: Date.now(),
        }])
    }, [])

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault()
        if (!input.trim() || isExecuting) return

        const command = input.trim()
        setInput('')
        setCommandHistory(prev => [...prev, command])
        setHistoryIndex(-1)

        addToHistory('input', command)

        // Handle local commands
        if (command === 'clear') {
            setHistory([])
            return
        }

        if (command === 'help') {
            addToHistory('output', HELP_TEXT)
            return
        }

        if (connectionStatus !== 'connected') {
            addToHistory('error', 'Not connected to server. Please wait for connection...')
            return
        }

        setIsExecuting(true)
        try {
            const result = await executeCommand(command)
            const output = typeof result === 'string' ? result : JSON.stringify(result, null, 2)
            addToHistory('output', output)
        } catch (err) {
            addToHistory('error', err instanceof Error ? err.message : 'Command failed')
        } finally {
            setIsExecuting(false)
            inputRef.current?.focus()
        }
    }

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'ArrowUp') {
            e.preventDefault()
            if (commandHistory.length > 0) {
                const newIndex = historyIndex < commandHistory.length - 1 ? historyIndex + 1 : historyIndex
                setHistoryIndex(newIndex)
                setInput(commandHistory[commandHistory.length - 1 - newIndex] || '')
            }
        } else if (e.key === 'ArrowDown') {
            e.preventDefault()
            if (historyIndex > 0) {
                const newIndex = historyIndex - 1
                setHistoryIndex(newIndex)
                setInput(commandHistory[commandHistory.length - 1 - newIndex] || '')
            } else if (historyIndex === 0) {
                setHistoryIndex(-1)
                setInput('')
            }
        }
    }

    const copyOutput = () => {
        const output = history
            .map(h => h.type === 'input' ? `$ ${h.content}` : h.content)
            .join('\n')
        navigator.clipboard.writeText(output)
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
    }

    const downloadOutput = () => {
        const output = history
            .map(h => h.type === 'input' ? `$ ${h.content}` : h.content)
            .join('\n')
        const blob = new Blob([output], { type: 'text/plain' })
        const url = URL.createObjectURL(blob)
        const a = document.createElement('a')
        a.href = url
        a.download = `aethershell-session-${new Date().toISOString().slice(0, 10)}.txt`
        a.click()
        URL.revokeObjectURL(url)
    }

    return (
        <div className="space-y-6 animate-fade-in h-[calc(100vh-8rem)]">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-2xl font-bold text-text">Terminal</h1>
                    <p className="text-subtext0 mt-1">Execute AetherShell commands directly</p>
                </div>
                <div className="flex gap-2">
                    <Button variant="ghost" size="sm" onClick={copyOutput}>
                        {copied ? <Check size={16} /> : <Copy size={16} />}
                        {copied ? 'Copied!' : 'Copy'}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={downloadOutput}>
                        <Download size={16} />
                        Export
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => setHistory([])}>
                        <Trash2 size={16} />
                        Clear
                    </Button>
                </div>
            </div>

            {/* Terminal */}
            <Card className="flex-1 flex flex-col h-full" padding={false}>
                {/* Terminal header */}
                <div className="flex items-center gap-2 px-4 py-2 bg-surface1/50 border-b border-surface1">
                    <div className="flex gap-1.5">
                        <div className="w-3 h-3 rounded-full bg-red/50" />
                        <div className="w-3 h-3 rounded-full bg-yellow/50" />
                        <div className="w-3 h-3 rounded-full bg-green/50" />
                    </div>
                    <span className="text-sm text-subtext0 ml-2">AetherShell Terminal</span>
                    <div className="ml-auto flex items-center gap-2">
                        <div className={clsx(
                            'w-2 h-2 rounded-full',
                            connectionStatus === 'connected' ? 'bg-green' :
                                connectionStatus === 'connecting' ? 'bg-yellow animate-pulse' :
                                    'bg-red'
                        )} />
                        <span className="text-xs text-subtext0">{connectionStatus}</span>
                    </div>
                </div>

                {/* Terminal content */}
                <div
                    ref={terminalRef}
                    className="flex-1 overflow-auto p-4 font-mono text-sm"
                    onClick={() => inputRef.current?.focus()}
                >
                    {/* Welcome message */}
                    {history.length === 0 && (
                        <div className="text-subtext0 mb-4">
                            <pre className="text-mauve">{ASCII_LOGO}</pre>
                            <p className="mt-4">Welcome to AetherShell Terminal!</p>
                            <p>Type <span className="text-mauve">help</span> for available commands.</p>
                        </div>
                    )}

                    {/* History */}
                    {history.map((entry) => (
                        <div key={entry.id} className="mb-2">
                            {entry.type === 'input' ? (
                                <div className="flex items-center gap-2">
                                    <span className="text-mauve">❯</span>
                                    <span className="text-text">{entry.content}</span>
                                </div>
                            ) : entry.type === 'error' ? (
                                <pre className="text-red whitespace-pre-wrap">{entry.content}</pre>
                            ) : (
                                <pre className="text-subtext0 whitespace-pre-wrap">{entry.content}</pre>
                            )}
                        </div>
                    ))}

                    {/* Loading indicator */}
                    {isExecuting && (
                        <div className="flex items-center gap-2 text-subtext0">
                            <Loader2 size={14} className="animate-spin" />
                            <span>Executing...</span>
                        </div>
                    )}
                </div>

                {/* Input area */}
                <form onSubmit={handleSubmit} className="border-t border-surface1 p-3">
                    <div className="flex items-center gap-2">
                        <span className="text-mauve">❯</span>
                        <input
                            ref={inputRef}
                            type="text"
                            value={input}
                            onChange={(e) => setInput(e.target.value)}
                            onKeyDown={handleKeyDown}
                            disabled={isExecuting}
                            placeholder="Enter AetherShell command..."
                            className="flex-1 bg-transparent text-text placeholder:text-subtext0 focus:outline-none font-mono"
                            autoComplete="off"
                            spellCheck={false}
                        />
                        <Button size="sm" disabled={isExecuting || !input.trim()}>
                            {isExecuting ? <Loader2 size={14} className="animate-spin" /> : <Send size={14} />}
                        </Button>
                    </div>
                </form>
            </Card>
        </div>
    )
}

const ASCII_LOGO = `
    ___       __  __            _____ __         ____
   /   | ____/ /_/ /_  ___  ____ / ___// /_  ___  / / /
  / /| |/ _ \\/ __/ __ \\/ _ \\/ ___/\\__ \\/ __ \\/ _ \\/ / / 
 / ___ /  __/ /_/ / / /  __/ /  ___/ / / / /  __/ / /  
/_/  |_\\___/\\__/_/ /_/\\___/_/  /____/_/ /_/\\___/_/_/   
`

const HELP_TEXT = `
AetherShell Terminal - Available Commands

BASICS:
  let x = 42              Declare a variable
  [1, 2, 3]               Array literal
  { name: "test" }        Record literal
  fn(x) => x * 2          Lambda function

PIPELINES:
  [1,2,3] | map(fn(x) => x * 2)
  ls "." | where(fn(f) => f.size > 1000)
  
AI FEATURES:
  ai("What is Rust?")     Query AI model
  agent("system prompt")  Create an agent

BUILTINS:
  ls, cd, pwd, cat, env, print, len, sum, 
  map, filter, reduce, sort, reverse, take, skip,
  http_get, http_post, json_parse, json_stringify

TERMINAL:
  clear                   Clear terminal
  help                    Show this help

For more, see: https://github.com/nervosys/AetherShell
`
