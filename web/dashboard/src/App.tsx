import { Routes, Route } from 'react-router-dom'
import Layout from './components/Layout'
import Dashboard from './pages/Dashboard'
import Agents from './pages/Agents'
import Workflows from './pages/Workflows'
import Metrics from './pages/Metrics'
import Marketplace from './pages/Marketplace'
import Terminal from './pages/Terminal'

function App() {
    return (
        <Routes>
            <Route path="/" element={<Layout />}>
                <Route index element={<Dashboard />} />
                <Route path="agents" element={<Agents />} />
                <Route path="workflows" element={<Workflows />} />
                <Route path="metrics" element={<Metrics />} />
                <Route path="marketplace" element={<Marketplace />} />
                <Route path="terminal" element={<Terminal />} />
            </Route>
        </Routes>
    )
}

export default App
