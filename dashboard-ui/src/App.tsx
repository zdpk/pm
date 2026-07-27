import ArrowUpRightIcon from 'lucide-react/dist/esm/icons/arrow-up-right'
import BoxIcon from 'lucide-react/dist/esm/icons/box'
import CircleAlertIcon from 'lucide-react/dist/esm/icons/circle-alert'
import PauseIcon from 'lucide-react/dist/esm/icons/pause'
import PlayIcon from 'lucide-react/dist/esm/icons/play'
import RefreshCwIcon from 'lucide-react/dist/esm/icons/refresh-cw'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Separator } from '@/components/ui/separator'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  getFrontendPresentation,
  getNavigationTarget,
  getRouteWarning,
  isProcessRunning,
  type DashboardApp,
  type DashboardSnapshot,
  type NavigationTarget,
} from '@/dashboard'
import { useDashboard } from '@/use-dashboard'

function SummaryMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="min-w-24 text-center">
      <strong className="block text-2xl font-semibold tabular-nums">{value}</strong>
      <span className="text-xs text-muted-foreground">{label}</span>
    </div>
  )
}

function Summary({ snapshot }: { snapshot: DashboardSnapshot }) {
  return (
    <div
      className="col-span-1 flex items-stretch justify-center gap-3 border-t pt-4 min-[380px]:col-span-2 lg:col-span-1 lg:col-start-2 lg:row-start-1 lg:border-t-0 lg:pt-0"
      aria-label="Process summary"
    >
      <SummaryMetric label="background" value={snapshot.background_count} />
      <Separator orientation="vertical" />
      <SummaryMetric label="foreground" value={snapshot.foreground_count} />
      <Separator orientation="vertical" />
      <SummaryMetric label="need attention" value={snapshot.attention_count} />
    </div>
  )
}

interface DashboardHeaderProps {
  snapshot: DashboardSnapshot | null
  isPaused: boolean
  isRefreshing: boolean
  onTogglePaused: () => void
  onRefresh: () => void
}

function DashboardHeader({
  snapshot,
  isPaused,
  isRefreshing,
  onTogglePaused,
  onRefresh,
}: DashboardHeaderProps) {
  return (
    <header className="border-b">
      <div className="mx-auto grid max-w-screen-2xl grid-cols-1 items-center gap-4 px-5 py-5 min-[380px]:grid-cols-[minmax(0,1fr)_auto] sm:px-8 lg:grid-cols-[minmax(0,1fr)_auto_auto] lg:px-10">
        <h1 className="text-xl font-semibold tracking-tight min-[420px]:text-2xl sm:text-3xl">
          PM Processes
        </h1>
        <div className="flex flex-wrap gap-2 min-[380px]:col-start-2 min-[380px]:row-start-1 lg:col-start-3">
          <Button
            variant="outline"
            size="lg"
            aria-pressed={isPaused}
            onClick={onTogglePaused}
          >
            {isPaused ? (
              <PlayIcon data-icon="inline-start" />
            ) : (
              <PauseIcon data-icon="inline-start" />
            )}
            {isPaused ? 'Resume' : 'Pause'}
          </Button>
          <Button
            variant="outline"
            size="lg"
            aria-busy={isRefreshing}
            disabled={isRefreshing}
            onClick={onRefresh}
          >
            <RefreshCwIcon data-icon="inline-start" />
            Refresh
          </Button>
        </div>
        {snapshot ? <Summary snapshot={snapshot} /> : (
          <div className="col-span-1 flex justify-center gap-4 border-t pt-4 min-[380px]:col-span-2 lg:col-span-1 lg:col-start-2 lg:row-start-1 lg:border-t-0 lg:pt-0">
            <Skeleton className="h-12 w-24" />
            <Skeleton className="h-12 w-24" />
            <Skeleton className="h-12 w-24" />
          </div>
        )}
      </div>
    </header>
  )
}

function ProcessBadges({ app }: { app: DashboardApp }) {
  const running = isProcessRunning(app.status)
  const routeWarning = getRouteWarning(app.status)

  return (
    <div className="flex max-w-36 flex-wrap justify-end gap-1.5">
      <Badge variant={running ? 'default' : 'outline'}>
        {running ? 'Running' : 'Stopped'}
      </Badge>
      {routeWarning ? (
        <Badge variant="destructive">{routeWarning}</Badge>
      ) : null}
      <Badge variant="outline">
        {app.mode === 'background' ? 'Background' : 'Foreground'}
      </Badge>
    </div>
  )
}

function FrontendVisual({
  displayName,
  imagePath,
}: {
  displayName: string
  imagePath: string | null
}) {
  return (
    <div className="aspect-[4/3] overflow-hidden bg-muted lg:row-span-3 lg:aspect-auto">
      {imagePath ? (
        <img
          className="size-full object-cover"
          src={imagePath}
          alt=""
          width="1024"
          height="1024"
          decoding="async"
        />
      ) : (
        <div
          className="flex size-full flex-col items-center justify-center gap-2 text-muted-foreground"
          aria-hidden="true"
        >
          <BoxIcon className="size-8" />
          <span className="text-sm">{displayName}</span>
        </div>
      )}
    </div>
  )
}

function ProcessMetadata({ app }: { app: DashboardApp }) {
  return (
    <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-xs sm:grid-cols-[0.7fr_0.7fr_1.6fr]">
      <div>
        <dt className="uppercase tracking-wide text-muted-foreground">Port</dt>
        <dd className="mt-1 font-medium tabular-nums">
          {app.upstream_port ?? '—'}
        </dd>
      </div>
      <div>
        <dt className="uppercase tracking-wide text-muted-foreground">PID</dt>
        <dd className="mt-1 font-medium tabular-nums">{app.pid}</dd>
      </div>
      <div className="col-span-2 sm:col-span-1">
        <dt className="uppercase tracking-wide text-muted-foreground">Started</dt>
        <dd className="mt-1 font-medium">{app.started_at}</dd>
      </div>
    </dl>
  )
}

function navigationActionLabel(navigation: NavigationTarget | null): string {
  if (!navigation) {
    return 'Route unavailable'
  }

  let port = ''
  try {
    port = new URL(navigation.url).port
  } catch {
    // The full target remains on the card; a malformed URL simply omits its port label.
  }
  return `Open via ${navigation.kind}${port ? ` :${port}` : ''}`
}

function FrontendCardBody({ app }: { app: DashboardApp }) {
  const presentation = getFrontendPresentation(app)
  const navigation = getNavigationTarget(app)

  return (
    <Card className="h-full gap-0 py-0 transition-colors group-hover:bg-accent/30 group-focus-visible:ring-2 group-focus-visible:ring-ring lg:grid lg:grid-cols-[minmax(8rem,0.62fr)_minmax(0,1fr)]">
      <FrontendVisual
        displayName={presentation.displayName}
        imagePath={presentation.imagePath}
      />
      <CardHeader className="py-5">
        <CardTitle className="flex flex-col gap-2 text-xl">
          <span className="font-mono text-xs font-normal text-muted-foreground">
            {app.workspace} / {app.service}
          </span>
          <h3>{presentation.displayName}</h3>
        </CardTitle>
        <CardDescription className="col-span-2 mt-2">
          {presentation.description}
        </CardDescription>
        <CardAction className="row-span-1">
          <ProcessBadges app={app} />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 pb-5">
        <Separator />
        <ProcessMetadata app={app} />
      </CardContent>
      <CardFooter className="justify-between">
        <span className="font-medium">{navigationActionLabel(navigation)}</span>
        {navigation ? (
          <ArrowUpRightIcon className="size-4" aria-hidden="true" />
        ) : null}
      </CardFooter>
    </Card>
  )
}

function FrontendCard({ app }: { app: DashboardApp }) {
  const presentation = getFrontendPresentation(app)
  const navigation = getNavigationTarget(app)

  if (!navigation) {
    return (
      <article aria-label={`${presentation.displayName} is unavailable`}>
        <FrontendCardBody app={app} />
      </article>
    )
  }

  return (
    <a
      className="group block rounded-xl outline-none"
      href={navigation.url}
      aria-label={`Open ${presentation.displayName} via ${navigation.kind}`}
    >
      <FrontendCardBody app={app} />
    </a>
  )
}

function FrontendEmpty() {
  return (
    <Empty className="min-h-56 border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <BoxIcon />
        </EmptyMedia>
        <EmptyTitle>No frontend applications</EmptyTitle>
        <EmptyDescription>
          PM is not currently managing a frontend service.
        </EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}

function FrontendSection({ apps }: { apps: DashboardApp[] }) {
  return (
    <section aria-labelledby="frontends-title">
      <h2 id="frontends-title" className="mb-4 text-xl font-semibold">
        Frontend applications
      </h2>
      {apps.length > 0 ? (
        <div className="grid gap-5 xl:grid-cols-2 min-[87.5rem]:grid-cols-3">
          {apps.map((app) => (
            <FrontendCard
              key={`${app.workspace}/${app.project}/${app.service}/${app.pid}`}
              app={app}
            />
          ))}
        </div>
      ) : (
        <FrontendEmpty />
      )}
    </section>
  )
}

function SupportingStatus({ app }: { app: DashboardApp }) {
  const running = isProcessRunning(app.status)
  const routeWarning = getRouteWarning(app.status)
  return (
    <div className="flex flex-wrap gap-1.5">
      <Badge variant={running ? 'default' : 'outline'}>
        {running ? 'Running' : 'Stopped'}
      </Badge>
      {routeWarning ? (
        <Badge variant="destructive">{routeWarning}</Badge>
      ) : null}
      <Badge variant="outline">
        {app.mode === 'background' ? 'Background' : 'Foreground'}
      </Badge>
    </div>
  )
}

function SupportingDesktopTable({ apps }: { apps: DashboardApp[] }) {
  return (
    <div className="hidden overflow-hidden rounded-xl border md:block">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Workspace / service</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Mode</TableHead>
            <TableHead>Port</TableHead>
            <TableHead>PID</TableHead>
            <TableHead>Started</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {apps.map((app) => (
            <TableRow
              key={`${app.workspace}/${app.project}/${app.service}/${app.pid}`}
            >
              <TableCell className="font-medium">
                {app.workspace} / {app.project} / {app.service}
              </TableCell>
              <TableCell>
                <div className="flex flex-wrap gap-1.5">
                  <Badge
                    variant={isProcessRunning(app.status) ? 'default' : 'outline'}
                  >
                    {isProcessRunning(app.status) ? 'Running' : 'Stopped'}
                  </Badge>
                  {getRouteWarning(app.status) ? (
                    <Badge variant="destructive">
                      {getRouteWarning(app.status)}
                    </Badge>
                  ) : null}
                </div>
              </TableCell>
              <TableCell>
                <Badge variant="outline">
                  {app.mode === 'background' ? 'Background' : 'Foreground'}
                </Badge>
              </TableCell>
              <TableCell className="tabular-nums">
                {app.upstream_port ?? '—'}
              </TableCell>
              <TableCell className="tabular-nums">{app.pid}</TableCell>
              <TableCell>{app.started_at}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}

function SupportingMobileList({ apps }: { apps: DashboardApp[] }) {
  return (
    <div className="rounded-xl border md:hidden">
      {apps.map((app, index) => (
        <div
          className="flex flex-col gap-4 p-4"
          key={`${app.workspace}/${app.project}/${app.service}/${app.pid}`}
        >
          {index > 0 ? <Separator className="-mt-4" /> : null}
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <p className="truncate font-medium">
                {app.project} / {app.service}
              </p>
              <p className="text-xs text-muted-foreground">{app.workspace}</p>
            </div>
            <SupportingStatus app={app} />
          </div>
          <ProcessMetadata app={app} />
        </div>
      ))}
    </div>
  )
}

function SupportingSection({ apps }: { apps: DashboardApp[] }) {
  if (apps.length === 0) {
    return null
  }

  return (
    <section aria-labelledby="supporting-title">
      <h2 id="supporting-title" className="mb-4 text-xl font-semibold">
        Backend &amp; other processes
      </h2>
      <SupportingDesktopTable apps={apps} />
      <SupportingMobileList apps={apps} />
    </section>
  )
}

function DashboardEmpty({
  error,
  onRetry,
}: {
  error?: string
  onRetry: () => void
}) {
  return (
    <Empty className="min-h-96 border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          {error ? <CircleAlertIcon /> : <BoxIcon />}
        </EmptyMedia>
        <EmptyTitle>
          {error ? 'Dashboard unavailable' : 'No PM processes are running'}
        </EmptyTitle>
        <EmptyDescription>
          {error ??
            'Start an application through PM and it will appear here automatically.'}
        </EmptyDescription>
      </EmptyHeader>
      {error ? (
        <EmptyContent>
          <Button variant="outline" onClick={onRetry}>
            <RefreshCwIcon data-icon="inline-start" />
            Try again
          </Button>
        </EmptyContent>
      ) : null}
    </Empty>
  )
}

function DashboardSkeleton() {
  return (
    <div
      className="grid gap-5 xl:grid-cols-2 min-[87.5rem]:grid-cols-3"
      aria-label="Dashboard loading"
      aria-busy="true"
    >
      {[0, 1, 2].map((item) => (
        <Card className="gap-0 py-0 lg:grid lg:grid-cols-2" key={item}>
          <Skeleton className="aspect-[4/3] rounded-none lg:aspect-auto" />
          <div className="flex flex-col gap-4 p-5">
            <Skeleton className="h-3 w-24" />
            <Skeleton className="h-6 w-36" />
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-20 w-full" />
          </div>
        </Card>
      ))}
    </div>
  )
}

function DashboardContent({
  snapshot,
  error,
  isLoading,
  onRetry,
}: {
  snapshot: DashboardSnapshot | null
  error: string | null
  isLoading: boolean
  onRetry: () => void
}) {
  if (isLoading) {
    return (
      <section aria-labelledby="frontends-title">
        <h2 id="frontends-title" className="mb-4 text-xl font-semibold">
          Frontend applications
        </h2>
        <DashboardSkeleton />
      </section>
    )
  }
  if (error) {
    return <DashboardEmpty error={error} onRetry={onRetry} />
  }
  if (!snapshot || snapshot.apps.length === 0) {
    return <DashboardEmpty onRetry={onRetry} />
  }

  const frontends = snapshot.apps.filter((app) => app.kind === 'frontend')
  const supporting = snapshot.apps.filter((app) => app.kind !== 'frontend')

  return (
    <>
      <FrontendSection apps={frontends} />
      <SupportingSection apps={supporting} />
    </>
  )
}

function App() {
  const {
    data,
    error,
    isLoading,
    isRefreshing,
    isPaused,
    refresh,
    togglePaused,
  } = useDashboard()

  return (
    <div className="min-h-screen">
      <DashboardHeader
        snapshot={data}
        isPaused={isPaused}
        isRefreshing={isRefreshing}
        onTogglePaused={togglePaused}
        onRefresh={() => void refresh()}
      />
      <main className="mx-auto flex max-w-screen-2xl flex-col gap-8 px-5 py-7 sm:px-8 lg:px-10 lg:py-9">
        <DashboardContent
          snapshot={data}
          error={error}
          isLoading={isLoading}
          onRetry={() => void refresh()}
        />
      </main>
    </div>
  )
}

export default App
