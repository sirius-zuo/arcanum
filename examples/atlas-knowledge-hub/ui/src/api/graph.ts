import { useQuery } from '@tanstack/react-query'
import { useBootstrap } from '../state/bootstrap'
import type { Client } from './client'
import type { GraphView } from './types'

export function getGraph(client: Client, collection: string): Promise<GraphView> {
  return client.get<GraphView>(`/api/v1/graph?collection_id=${encodeURIComponent(collection)}`)
}

export function useGraph() {
  const { data, client } = useBootstrap()
  return useQuery({
    queryKey: ['graph', data?.collection],
    queryFn: () => getGraph(client, data!.collection),
    enabled: data !== null,
    retry: false,
  })
}
