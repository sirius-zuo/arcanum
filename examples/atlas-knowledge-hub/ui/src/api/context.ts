import { useMutation } from '@tanstack/react-query'
import { useBootstrap } from '../state/bootstrap'
import type { Client } from './client'
import type { ContextRequest, ContextResponse } from './types'

export function buildContext(client: Client, req: ContextRequest): Promise<ContextResponse> {
  return client.post<ContextResponse>('/api/v1/context', req)
}

export function useBuildContext() {
  const { client } = useBootstrap()
  return useMutation({ mutationFn: (req: ContextRequest) => buildContext(client, req) })
}
