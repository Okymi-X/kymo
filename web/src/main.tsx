import { StrictMode, Suspense, lazy } from 'react'
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
import './index.css'

// Each page loads when it is visited, so the first screen does not pay for the whole app.
const Ladder = lazy(() => import('./pages/Ladder').then((m) => ({ default: m.Ladder })))
const AdminEvent = lazy(() => import('./pages/admin/AdminEvent').then((m) => ({ default: m.AdminEvent })))
const AdminEvents = lazy(() => import('./pages/admin/AdminEvents').then((m) => ({ default: m.AdminEvents })))
const AdminUsers = lazy(() => import('./pages/admin/AdminUsers').then((m) => ({ default: m.AdminUsers })))
const Ops = lazy(() => import('./pages/admin/Ops').then((m) => ({ default: m.Ops })))
const Monitor = lazy(() => import('./pages/Monitor').then((m) => ({ default: m.Monitor })))
const Events = lazy(() => import('./pages/Events').then((m) => ({ default: m.Events })))
const EventPage = lazy(() => import('./pages/EventPage').then((m) => ({ default: m.EventPage })))
const Profile = lazy(() => import('./pages/Profile').then((m) => ({ default: m.Profile })))
const SignIn = lazy(() => import('./pages/SignIn').then((m) => ({ default: m.SignIn })))
const TeamPage = lazy(() => import('./pages/TeamPage').then((m) => ({ default: m.TeamPage })))
const Login = lazy(() => import('./pages/Login').then((m) => ({ default: m.Login })))
const Settings = lazy(() => import('./pages/Settings').then((m) => ({ default: m.Settings })))
const Tokens = lazy(() => import('./pages/Tokens').then((m) => ({ default: m.Tokens })))
const EmbedPage = lazy(() => import('./pages/EmbedPage').then((m) => ({ default: m.EmbedPage })))
const ReportPage = lazy(() => import('./pages/ReportPage').then((m) => ({ default: m.ReportPage })))
const StagePage = lazy(() => import('./pages/StagePage').then((m) => ({ default: m.StagePage })))
const TournamentPage = lazy(() => import('./pages/TournamentPage').then((m) => ({ default: m.TournamentPage })))

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
            <Route path="/t/:slug/stage" element={<Suspense fallback={null}><StagePage /></Suspense>} />
            <Route path="/embed/:slug" element={<Suspense fallback={null}><EmbedPage /></Suspense>} />
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
