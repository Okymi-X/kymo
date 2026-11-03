import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MotionConfig } from 'motion/react'
import { BrowserRouter, Route, Routes } from 'react-router'
import { ThemeLoader, appearanceKey } from './components/ThemeLoader'
import type { Appearance } from './api/types'
import { resolveScheme } from './lib/scheme'
import { Shell } from './components/Shell'
import { Home } from './pages/Home'
import { NotFound } from './pages/NotFound'
import { Ladder } from './pages/Ladder'
import { AdminEvent } from './pages/admin/AdminEvent'
import { AdminEvents } from './pages/admin/AdminEvents'
import { AdminUsers } from './pages/admin/AdminUsers'
import { Ops } from './pages/admin/Ops'
import { Monitor } from './pages/Monitor'
import { Events } from './pages/Events'
import { EventPage } from './pages/EventPage'
import { Profile } from './pages/Profile'
import { SignIn } from './pages/SignIn'
import { TeamPage } from './pages/TeamPage'
import { Login } from './pages/Login'
import { Settings } from './pages/Settings'
import { Tokens } from './pages/Tokens'
import { EmbedPage } from './pages/EmbedPage'
import { ReportPage } from './pages/ReportPage'
import { StagePage } from './pages/StagePage'
import { TournamentPage } from './pages/TournamentPage'
import './index.css'

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 5_000 } } })

/** The server inlines the saved look in index.html, so the first render needs no request. */
function seedAppearance() {
  try {
    const raw = document.getElementById('kymo-appearance')?.textContent
    if (raw) queryClient.setQueryData<Appearance>(appearanceKey, JSON.parse(raw) as Appearance)
  } catch {
    /* fall back to fetching /settings */
  }
}
seedAppearance()
{
  // Apply the visitor's own light or dark choice before the first render, unless the site forces one.
  const forced = queryClient.getQueryData<Appearance>(appearanceKey)?.scheme ?? 'system'
  document.documentElement.dataset.scheme = resolveScheme(forced)
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <MotionConfig reducedMotion="user">
        <BrowserRouter>
          <ThemeLoader />
          <Routes>
            <Route path="/t/:slug/stage" element={<StagePage />} />
            <Route path="/embed/:slug" element={<EmbedPage />} />
            <Route element={<Shell />}>
              <Route path="/" element={<Home />} />
              <Route path="/login" element={<Login />} />
              <Route path="/signin" element={<SignIn />} />
              <Route path="/events" element={<Events />} />
              <Route path="/e/:slug" element={<EventPage />} />
              <Route path="/team" element={<TeamPage />} />
              <Route path="/profile" element={<Profile />} />
              <Route path="/admin/events" element={<AdminEvents />} />
              <Route path="/admin/events/:id" element={<AdminEvent />} />
              <Route path="/admin/users" element={<AdminUsers />} />
              <Route path="/admin/ops" element={<Ops />} />
              <Route path="/monitor" element={<Monitor />} />
              <Route path="/ladder" element={<Ladder />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="/tokens" element={<Tokens />} />
              <Route path="/t/:slug" element={<TournamentPage />} />
              <Route path="/report/:code" element={<ReportPage />} />
              <Route path="*" element={<NotFound />} />
            </Route>
          </Routes>
        </BrowserRouter>
      </MotionConfig>
    </QueryClientProvider>
  </StrictMode>,
)
