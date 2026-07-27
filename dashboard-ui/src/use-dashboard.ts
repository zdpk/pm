import { useCallback, useEffect, useRef, useState } from 'react'

import {
  parseDashboardSnapshot,
  type DashboardSnapshot,
} from '@/dashboard'

const POLL_INTERVAL_MS = 5_000

function hasInteractiveFocus(): boolean {
  const activeElement = document.activeElement
  return (
    activeElement !== null &&
    activeElement !== document.body &&
    activeElement !== document.documentElement
  )
}

interface DashboardState {
  data: DashboardSnapshot | null
  error: string | null
  isLoading: boolean
  isRefreshing: boolean
}

const INITIAL_STATE: DashboardState = {
  data: null,
  error: null,
  isLoading: true,
  isRefreshing: false,
}

export function useDashboard() {
  const [state, setState] = useState<DashboardState>(INITIAL_STATE)
  const [isPaused, setIsPaused] = useState(false)
  const requestRef = useRef<AbortController | null>(null)

  const refresh = useCallback(async () => {
    requestRef.current?.abort()
    const controller = new AbortController()
    requestRef.current = controller

    setState((current) => ({
      ...current,
      error: null,
      isRefreshing: current.data !== null,
    }))

    try {
      const response = await fetch('/api/dashboard', {
        headers: { accept: 'application/json' },
        signal: controller.signal,
      })
      if (!response.ok) {
        throw new Error(`Dashboard request failed (${response.status}).`)
      }
      const data = parseDashboardSnapshot(await response.json())
      setState({
        data,
        error: null,
        isLoading: false,
        isRefreshing: false,
      })
    } catch (error) {
      if (controller.signal.aborted) {
        return
      }
      setState({
        data: null,
        error:
          error instanceof Error
            ? error.message
            : 'The dashboard could not be loaded.',
        isLoading: false,
        isRefreshing: false,
      })
    }
  }, [])

  useEffect(() => {
    void refresh()
    return () => {
      requestRef.current?.abort()
    }
  }, [refresh])

  useEffect(() => {
    let timer: number | undefined
    let cancelled = false

    const schedule = () => {
      window.clearTimeout(timer)
      timer = undefined

      if (
        cancelled ||
        isPaused ||
        document.visibilityState !== 'visible'
      ) {
        return
      }

      timer = window.setTimeout(() => {
        if (hasInteractiveFocus()) {
          schedule()
          return
        }
        void refresh().finally(schedule)
      }, POLL_INTERVAL_MS)
    }

    const reschedule = () => schedule()
    document.addEventListener('visibilitychange', reschedule)
    document.addEventListener('focusin', reschedule)
    document.addEventListener('focusout', reschedule)
    schedule()

    return () => {
      cancelled = true
      window.clearTimeout(timer)
      document.removeEventListener('visibilitychange', reschedule)
      document.removeEventListener('focusin', reschedule)
      document.removeEventListener('focusout', reschedule)
    }
  }, [isPaused, refresh])

  const togglePaused = useCallback(() => {
    setIsPaused((current) => !current)
  }, [])

  return {
    ...state,
    isPaused,
    refresh,
    togglePaused,
  }
}
