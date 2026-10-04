import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useBootstrap } from '../state/bootstrap'
import { useOperations } from '../state/operations'
import type { DemoHealth, DocumentText, Library, Samples, SamplesOperations } from './types'

/** Poll quickly while something is wrong, slowly once everything is ready. */
export function healthRefetchInterval(data: DemoHealth | undefined): number {
  return data?.ready ? 30_000 : 5_000
}

export function useHealth() {
  const { data, client } = useBootstrap()
  return useQuery({
    queryKey: ['demo', 'health'],
    queryFn: () => client.get<DemoHealth>('/demo/health'),
    enabled: data !== null,
    refetchInterval: (query) => healthRefetchInterval(query.state.data),
    retry: false,
  })
}

export function useSamples() {
  const { data, client } = useBootstrap()
  return useQuery({
    queryKey: ['demo', 'samples'],
    queryFn: () => client.get<Samples>('/demo/samples'),
    enabled: data !== null,
    staleTime: Infinity,
  })
}

export function useLibrary() {
  const { data, client } = useBootstrap()
  return useQuery({
    queryKey: ['demo', 'library'],
    queryFn: () => client.get<Library>('/demo/library'),
    enabled: data !== null,
    retry: false,
  })
}

export function useDocumentText(documentId: string, versionNum: number) {
  const { data, client } = useBootstrap()
  return useQuery({
    queryKey: ['demo', 'document-text', documentId, versionNum],
    queryFn: () => client.get<DocumentText>(`/demo/documents/${encodeURIComponent(documentId)}/versions/${versionNum}/text`),
    enabled: data !== null,
    staleTime: Infinity,
    retry: false,
  })
}

function useOperationsMutation(path: string) {
  const { client } = useBootstrap()
  const { track } = useOperations()
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: () => client.post<SamplesOperations>(path),
    onSuccess: (res) => {
      track(res.operations)
      void queryClient.invalidateQueries({ queryKey: ['demo', 'library'] })
    },
  })
}

export function useLoadSamples() {
  return useOperationsMutation('/demo/samples/load')
}

export function useApplyUpdate() {
  return useOperationsMutation('/demo/samples/apply-update')
}
