import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MotionConfig } from 'motion/react'
import { BrowserRouter, Route, Routes } from 'react-router'
import { Shell } from './components/Shell'
import { Home } from './pages/Home'
import { Login } from './pages/Login'
import { TournamentPage } from './pages/TournamentPage'
import './index.css'

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 5_000 } } })

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <MotionConfig reducedMotion="user">
        <BrowserRouter>
          <Shell>
            <Routes>
              <Route path="/" element={<Home />} />
              <Route path="/login" element={<Login />} />
              <Route path="/t/:slug" element={<TournamentPage />} />
            </Routes>
          </Shell>
        </BrowserRouter>
      </MotionConfig>
    </QueryClientProvider>
  </StrictMode>,
)
