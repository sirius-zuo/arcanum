import { createContext, useContext, useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { connectEvents, eventsUrl } from '../api/ws'
import type { WsStatus } from '../api/ws'
import { isTerminal, operationKey } from '../api/ingest'
import type { OperationDoc } from '../api/ingest'
import { useBootstrap } from './bootstrap'

const Ctx = createContext<WsStatus>('closed')

/**
 * One shared socket for every tracked operation. The server forwards all ingestion events to
 * every client, so events are matched by operation_id and only ever trigger a refetch;
 * polling stays the source of truth when the socket is retrying or closed.
 */
export function IngestEventsProvider({ children }: { children: ReactNode }) {
  const { data } = useBootstrap()
  const queryClient = useQueryClient()
  const [status, setStatus] = useState<WsStatus>('closed')
  const key = data?.api_key ?? null

  useEffect(() => {
    if (!key) return
    return connectEvents({
      url: eventsUrl(window.location),
      key,
      onStatus: setStatus,
      onEvent: (e) => {
        const id = typeof e === 'object' && e !== null ? (e as { operation_id?: unknown }).operation_id : undefined
        if (typeof id !== 'string') return
        const cached = queryClient.getQueryData<OperationDoc>(operationKey(id))
        // Unknown ids belong to other tabs or clients; finished ones need nothing.
        if (cached === undefined && !queryClient.getQueryState(operationKey(id))) return
        if (isTerminal(cached)) return
        void queryClient.invalidateQueries({ queryKey: operationKey(id) })
      },
    })
  }, [key, queryClient])

  return <Ctx.Provider value={status}>{children}</Ctx.Provider>
}

/** `open` means events arrive live; anything else means the rail is relying on polling. */
export function useEventsStatus(): WsStatus {
  return useContext(Ctx)
}
