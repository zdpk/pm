export const DASHBOARD_SCHEMA_VERSION = 1 as const

export type ProcessKind =
  | 'frontend'
  | 'backend'
  | 'database'
  | 'redis'
  | 'infra'
  | null

export type ProcessMode = 'background' | 'foreground'

export type ProcessStatus =
  | 'running'
  | 'route_missing'
  | 'route_mismatch'
  | 'stale'

export interface DashboardApp {
  workspace: string
  project: string
  service: string
  kind: ProcessKind
  mode: ProcessMode
  pid: number
  upstream_port: number | null
  started_at: string
  url: string | null
  tailnet_urls: string[]
  status: ProcessStatus
}

export interface DashboardSnapshot {
  schema_version: typeof DASHBOARD_SCHEMA_VERSION
  apps: DashboardApp[]
  background_count: number
  foreground_count: number
  attention_count: number
}

export interface FrontendPresentation {
  displayName: string
  description: string
  imagePath: string | null
}

export interface NavigationTarget {
  kind: 'Tailnet' | 'Local'
  url: string
}

const PROCESS_KINDS = new Set([
  'frontend',
  'backend',
  'database',
  'redis',
  'infra',
])
const PROCESS_MODES = new Set(['background', 'foreground'])
const PROCESS_STATUSES = new Set([
  'running',
  'route_missing',
  'route_mismatch',
  'stale',
])

const FRONTEND_PRESENTATIONS = new Map<string, Omit<FrontendPresentation, 'displayName'>>([
  [
    'personal/i-learn/front',
    {
      description:
        'Cloud certification roadmaps, practice, AI tutoring and readiness.',
      imagePath: '/assets/dashboard/i-learn-card-v2.jpg',
    },
  ],
  [
    'personal/social-manager/front',
    {
      description:
        'Multi-brand publishing operations, account health and delivery queues across connected social channels.',
      imagePath: '/assets/dashboard/social-manager-card-v2.jpg',
    },
  ],
  [
    'personal/stock-analyzer/front',
    {
      description:
        'Evidence-backed futures flow and bio-pharma research.',
      imagePath: '/assets/dashboard/stock-analyzer-card-v2.jpg',
    },
  ],
  [
    'prod/korion/front',
    {
      description:
        'Visual vocabulary learning across Korean, Japanese and English.',
      imagePath: '/assets/dashboard/korion-card-v2.jpg',
    },
  ],
  [
    'prod/korion/studio',
    {
      description:
        'A private content studio for characters, word art, storybooks and social publishing.',
      imagePath: '/assets/dashboard/korion-studio-card-v2.jpg',
    },
  ],
  [
    'prod/topclass/front',
    {
      description:
        'Global rankings remixed into editorial cards and short-form stories.',
      imagePath: '/assets/dashboard/topclass-card-v2.jpg',
    },
  ],
])

const DISPLAY_NAMES = new Map([
  ['personal/social-manager/front', 'Social Manager'],
  ['prod/korion/studio', 'Korion Studio'],
])

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isNonNegativeInteger(value: unknown): value is number {
  return Number.isInteger(value) && Number(value) >= 0
}

function parseApp(value: unknown, index: number): DashboardApp {
  if (!isRecord(value)) {
    throw new Error(`Dashboard app ${index} is not an object.`)
  }

  const {
    workspace,
    project,
    service,
    kind,
    mode,
    pid,
    upstream_port: upstreamPort,
    started_at: startedAt,
    url,
    tailnet_urls: tailnetUrls,
    status,
  } = value

  if (
    typeof workspace !== 'string' ||
    typeof project !== 'string' ||
    typeof service !== 'string' ||
    (kind !== null && (typeof kind !== 'string' || !PROCESS_KINDS.has(kind))) ||
    typeof mode !== 'string' ||
    !PROCESS_MODES.has(mode) ||
    !isNonNegativeInteger(pid) ||
    (upstreamPort !== null && !isNonNegativeInteger(upstreamPort)) ||
    typeof startedAt !== 'string' ||
    (url !== null && typeof url !== 'string') ||
    !Array.isArray(tailnetUrls) ||
    !tailnetUrls.every((item) => typeof item === 'string') ||
    typeof status !== 'string' ||
    !PROCESS_STATUSES.has(status)
  ) {
    throw new Error(`Dashboard app ${index} does not match schema version 1.`)
  }

  return {
    workspace,
    project,
    service,
    kind: kind as ProcessKind,
    mode: mode as ProcessMode,
    pid,
    upstream_port: upstreamPort,
    started_at: startedAt,
    url,
    tailnet_urls: tailnetUrls,
    status: status as ProcessStatus,
  }
}

export function parseDashboardSnapshot(value: unknown): DashboardSnapshot {
  if (!isRecord(value)) {
    throw new Error('The dashboard response is not an object.')
  }

  const {
    schema_version: schemaVersion,
    apps,
    background_count: backgroundCount,
    foreground_count: foregroundCount,
    attention_count: attentionCount,
  } = value

  if (schemaVersion !== DASHBOARD_SCHEMA_VERSION) {
    throw new Error(`Unsupported dashboard schema version: ${String(schemaVersion)}.`)
  }
  if (!Array.isArray(apps)) {
    throw new Error('The dashboard response is missing its app list.')
  }
  if (
    !isNonNegativeInteger(backgroundCount) ||
    !isNonNegativeInteger(foregroundCount) ||
    !isNonNegativeInteger(attentionCount)
  ) {
    throw new Error('The dashboard response has invalid summary counts.')
  }

  return {
    schema_version: DASHBOARD_SCHEMA_VERSION,
    apps: apps.map(parseApp),
    background_count: backgroundCount,
    foreground_count: foregroundCount,
    attention_count: attentionCount,
  }
}

export function getFrontendPresentation(app: DashboardApp): FrontendPresentation {
  const identity = `${app.workspace}/${app.project}/${app.service}`
  const presentation = FRONTEND_PRESENTATIONS.get(identity)

  return {
    displayName: DISPLAY_NAMES.get(identity) ?? app.project,
    description:
      presentation?.description ?? 'A frontend application managed by PM.',
    imagePath: presentation?.imagePath ?? null,
  }
}

export function getNavigationTarget(app: DashboardApp): NavigationTarget | null {
  if (app.status === 'stale') {
    return null
  }

  const tailnetUrl = app.tailnet_urls[0]
  if (tailnetUrl) {
    return { kind: 'Tailnet', url: tailnetUrl }
  }
  if (app.url) {
    return { kind: 'Local', url: app.url }
  }
  return null
}

export function isProcessRunning(status: ProcessStatus): boolean {
  return status !== 'stale'
}

export function getRouteWarning(status: ProcessStatus): string | null {
  if (status === 'route_missing') {
    return 'Route missing'
  }
  if (status === 'route_mismatch') {
    return 'Route mismatch'
  }
  return null
}
