import { StrictMode, Suspense, lazy } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MotionConfig } from 'motion/react'
import { BrowserRouter, Route, Routes } from 'react-router'
import { ThemeLoader, appearanceKey } from './components/ThemeLoader'
import type { Appearance } from './api/types'
import { resolveScheme } from './lib/scheme'
import { RequireSession } from './components/RequireSession'
import { Shell } from './components/Shell'
import { OrganizerShell } from './components/organizer/OrganizerShell'
import { RequireStaff } from './components/organizer/RequireStaff'
import { LegacyRedirect } from './components/organizer/LegacyRedirect'
import { Home } from './pages/Home'
import { NotFound } from './pages/NotFound'
import './index.css'

// Each page loads when it is visited, so the first screen does not pay for the whole app.
const Ladder = lazy(() => import('./pages/Ladder').then((m) => ({ default: m.Ladder })))
const AdminEvent = lazy(() => import('./pages/admin/AdminEvent').then((m) => ({ default: m.AdminEvent })))
const AdminEvents = lazy(() => import('./pages/admin/AdminEvents').then((m) => ({ default: m.AdminEvents })))
const MatchPage = lazy(() => import('./pages/MatchPage').then((m) => ({ default: m.MatchPage })))
const AdminPermissions = lazy(() => import('./pages/admin/AdminPermissions').then((m) => ({ default: m.AdminPermissions })))
const AdminUsers = lazy(() => import('./pages/admin/AdminUsers').then((m) => ({ default: m.AdminUsers })))
const Labs = lazy(() => import('./pages/admin/Labs').then((m) => ({ default: m.Labs })))
const Ops = lazy(() => import('./pages/admin/Ops').then((m) => ({ default: m.Ops })))
const Monitor = lazy(() => import('./pages/Monitor').then((m) => ({ default: m.Monitor })))
const Events = lazy(() => import('./pages/Events').then((m) => ({ default: m.Events })))
const EventPage = lazy(() => import('./pages/EventPage').then((m) => ({ default: m.EventPage })))
const Profile = lazy(() => import('./pages/Profile').then((m) => ({ default: m.Profile })))
const PublicScoreboard = lazy(() => import('./pages/PublicScoreboard').then((m) => ({ default: m.PublicScoreboard })))
const SignIn = lazy(() => import('./pages/SignIn').then((m) => ({ default: m.SignIn })))
const TeamPage = lazy(() => import('./pages/TeamPage').then((m) => ({ default: m.TeamPage })))
const OrganizerLogin = lazy(() => import('./pages/organizer/OrganizerLogin').then((m) => ({ default: m.OrganizerLogin })))
const OrganizerHome = lazy(() => import('./pages/organizer/OrganizerHome').then((m) => ({ default: m.OrganizerHome })))
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
            <Route path="/organizer/login" element={<Suspense fallback={null}><OrganizerLogin /></Suspense>} />
            <Route element={<RequireStaff />}>
              <Route path="/organizer" element={<OrganizerShell />}>
                <Route index element={<OrganizerHome />} />
                <Route path="events" element={<AdminEvents />} />
                <Route path="events/:id" element={<AdminEvent />} />
                <Route path="users" element={<AdminUsers />} />
                <Route path="permissions" element={<AdminPermissions />} />
                <Route path="ops" element={<Ops />} />
                <Route path="labs" element={<Labs />} />
                <Route path="monitor" element={<Monitor />} />
                <Route path="appearance" element={<Settings />} />
                <Route path="tokens" element={<Tokens />} />
                <Route path="*" element={<NotFound />} />
              </Route>
            </Route>
            {/* The addresses the console had before it had its own area. */}
            {['/admin', '/admin/*', '/monitor', '/settings', '/tokens', '/login'].map((path) => (
              <Route key={path} path={path} element={<LegacyRedirect />} />
            ))}
            <Route element={<Shell />}>
              {/* Open to visitors: the front page, sign in, a public scoreboard and a team's score report (its code is its key). */}
              <Route path="/" element={<Home />} />
              <Route path="/signin" element={<SignIn />} />
              <Route path="/s/:slug" element={<PublicScoreboard />} />
              <Route path="/report/:code" element={<ReportPage />} />
              {/* Everything else needs a session. */}
              <Route element={<RequireSession />}>
                <Route path="/events" element={<Events />} />
                <Route path="/e/:slug" element={<EventPage />} />
                <Route path="/team" element={<TeamPage />} />
                <Route path="/profile" element={<Profile />} />
                <Route path="/ladder" element={<Ladder />} />
                <Route path="/t/:slug" element={<TournamentPage />} />
                <Route path="/t/:slug/match/:matchId" element={<MatchPage />} />
              </Route>
              <Route path="*" element={<NotFound />} />
            </Route>
          </Routes>
        </BrowserRouter>
      </MotionConfig>
    </QueryClientProvider>
  </StrictMode>,
)
