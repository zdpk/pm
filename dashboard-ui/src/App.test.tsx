import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import App from '@/App'
import {
  getFrontendPresentation,
  type DashboardApp,
  type DashboardSnapshot,
} from '@/dashboard'

const RUNNING_FRONTEND: DashboardApp = {
  workspace: 'personal',
  project: 'social-manager',
  service: 'front',
  kind: 'frontend',
  mode: 'background',
  pid: 78643,
  upstream_port: 17790,
  started_at: '2026-07-27 12:17:33 UTC',
  url: 'http://front.social-manager.personal.localhost:7100/',
  tailnet_urls: ['https://m5-air.tail4beb79.ts.net:14443/'],
  status: 'running',
}

function snapshot(
  apps: DashboardApp[],
  overrides: Partial<DashboardSnapshot> = {},
): DashboardSnapshot {
  return {
    schema_version: 1,
    apps,
    background_count: apps.filter((app) => app.mode === 'background').length,
    foreground_count: apps.filter((app) => app.mode === 'foreground').length,
    attention_count: apps.filter((app) => app.status !== 'running').length,
    ...overrides,
  }
}

function jsonResponse(data: DashboardSnapshot): Response {
  return {
    ok: true,
    json: async () => data,
  } as Response
}

describe('PM dashboard', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it('shows shadcn skeletons during the initial request', () => {
    vi.mocked(fetch).mockReturnValue(new Promise(() => {}))

    render(<App />)

    expect(screen.getByLabelText('Dashboard loading')).toHaveAttribute(
      'aria-busy',
      'true',
    )
    expect(document.querySelectorAll('[data-slot="skeleton"]')).not.toHaveLength(0)
  })

  it('prefers Tailnet for the full frontend card link', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse(snapshot([RUNNING_FRONTEND])))

    render(<App />)

    const cardLink = await screen.findByRole('link', {
      name: 'Open Social Manager via Tailnet',
    })
    expect(cardLink).toHaveAttribute(
      'href',
      'https://m5-air.tail4beb79.ts.net:14443/',
    )
    expect(cardLink).not.toHaveAttribute(
      'href',
      'http://front.social-manager.personal.localhost:7100/',
    )
    expect(within(cardLink).getByText('Running')).toBeVisible()
    expect(within(cardLink).getByText('Background')).toBeVisible()
    expect(within(cardLink).getByText('Open via Tailnet :14443')).toBeVisible()
  })

  it('retains the six curated frontend presentations', () => {
    const presentations = [
      [
        'personal',
        'i-learn',
        'front',
        'i-learn',
        '/assets/dashboard/i-learn-card-v2.jpg',
        'Cloud certification roadmaps, practice, AI tutoring and readiness.',
      ],
      [
        'personal',
        'social-manager',
        'front',
        'Social Manager',
        '/assets/dashboard/social-manager-card-v2.jpg',
        'Multi-brand publishing operations, account health and delivery queues across connected social channels.',
      ],
      [
        'personal',
        'stock-analyzer',
        'front',
        'stock-analyzer',
        '/assets/dashboard/stock-analyzer-card-v2.jpg',
        'Evidence-backed futures flow and bio-pharma research.',
      ],
      [
        'prod',
        'korion',
        'front',
        'korion',
        '/assets/dashboard/korion-card-v2.jpg',
        'Visual vocabulary learning across Korean, Japanese and English.',
      ],
      [
        'prod',
        'korion',
        'studio',
        'Korion Studio',
        '/assets/dashboard/korion-studio-card-v2.jpg',
        'A private content studio for characters, word art, storybooks and social publishing.',
      ],
      [
        'prod',
        'topclass',
        'front',
        'topclass',
        '/assets/dashboard/topclass-card-v2.jpg',
        'Global rankings remixed into editorial cards and short-form stories.',
      ],
    ] as const

    for (
      const [
        workspace,
        project,
        service,
        displayName,
        imagePath,
        description,
      ] of presentations
    ) {
      expect(
        getFrontendPresentation({
          ...RUNNING_FRONTEND,
          workspace,
          project,
          service,
        }),
      ).toEqual({ displayName, imagePath, description })
    }
  })

  it('renders stopped frontends as non-links', async () => {
    const stopped = {
      ...RUNNING_FRONTEND,
      project: 'i-learn',
      pid: 18841,
      status: 'stale' as const,
    }
    vi.mocked(fetch).mockResolvedValue(jsonResponse(snapshot([stopped])))

    render(<App />)

    const card = await screen.findByRole('article', {
      name: 'i-learn is unavailable',
    })
    expect(card.closest('a')).toBeNull()
    expect(within(card).getByText('Stopped')).toBeVisible()
    expect(within(card).getByText('Route unavailable')).toBeVisible()
  })

  it('shows a route warning separately from the running state', async () => {
    const routeMissing = {
      ...RUNNING_FRONTEND,
      status: 'route_missing' as const,
      url: null,
      tailnet_urls: [],
    }
    vi.mocked(fetch).mockResolvedValue(
      jsonResponse(snapshot([routeMissing], { attention_count: 1 })),
    )

    render(<App />)

    const card = await screen.findByRole('article', {
      name: 'Social Manager is unavailable',
    })
    expect(within(card).getByText('Running')).toBeVisible()
    expect(within(card).getByText('Route missing')).toBeVisible()
    expect(within(card).getByText('Background')).toBeVisible()
  })

  it('pauses automatic updates and supports a manual refresh', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    vi.mocked(fetch).mockResolvedValue(jsonResponse(snapshot([RUNNING_FRONTEND])))
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime })

    render(<App />)
    await screen.findByRole('link', {
      name: 'Open Social Manager via Tailnet',
    })

    await user.click(screen.getByRole('button', { name: 'Pause' }))
    expect(screen.getByRole('button', { name: 'Resume' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )

    await act(async () => {
      vi.advanceTimersByTime(5_000)
    })
    expect(fetch).toHaveBeenCalledTimes(1)

    await user.click(screen.getByRole('button', { name: 'Refresh' }))
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2))

    const resume = screen.getByRole('button', { name: 'Resume' })
    await user.click(resume)
    expect(resume).toHaveAttribute('aria-pressed', 'false')

    await act(async () => {
      vi.advanceTimersByTime(5_000)
    })
    expect(fetch).toHaveBeenCalledTimes(2)

    act(() => resume.blur())
    await act(async () => {
      vi.advanceTimersByTime(5_000)
    })
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(3))
  })

  it('uses the Empty component for empty and error responses', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(snapshot([])))

    const { unmount } = render(<App />)
    expect(
      await screen.findByText('No PM processes are running'),
    ).toBeVisible()
    expect(
      screen.getByText(
        'Start an application through PM and it will appear here automatically.',
      ),
    ).toBeVisible()
    unmount()

    vi.mocked(fetch).mockResolvedValueOnce({
      ok: false,
      status: 503,
    } as Response)
    render(<App />)

    expect(await screen.findByText('Dashboard unavailable')).toBeVisible()
    expect(screen.getByText('Dashboard request failed (503).')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Try again' })).toBeVisible()
  })
})
