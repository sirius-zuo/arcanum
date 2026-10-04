import type { Claim, ClaimEvidence, Verdict, VerifiedSentence, VerifyResponse } from '../api/types'

const enc = new TextEncoder()

/** UTF-8 byte span of `part` inside `whole`, the way the server reports it. */
export function byteSpan(whole: string, part: string): [number, number] {
  const idx = whole.indexOf(part)
  if (idx < 0) throw new Error(`"${part}" not found`)
  const start = enc.encode(whole.slice(0, idx)).length
  return [start, start + enc.encode(part).length]
}

export const DOC_ID = '11111111-1111-4111-8111-111111111111'
export const CHUNK_ID = '22222222-2222-4222-8222-222222222222'

export function evidence(over: Partial<ClaimEvidence> = {}): ClaimEvidence {
  return {
    ref_id: 'P1',
    chunk_id: CHUNK_ID,
    document_id: DOC_ID,
    version_num: 2,
    version_status: 'active',
    source_uri: 'hx2-datasheet.md',
    offset_start: 0,
    offset_end: 4,
    quote: 'Café',
    quote_matched: true,
    ...over,
  }
}

export function sentence(answer: string, text: string, verdict: Verdict, extra: Partial<VerifiedSentence> = {}, claims: Claim[] = []): VerifiedSentence {
  return { span: byteSpan(answer, text), text, verdict, cited: [], invalid_refs: [], claims, ...extra }
}

export const ANSWER = 'The HX-2 carries 60 kg [P1]. It also carries 80 kg [P1]. 日本語 is fine. Thanks!'

export function response(over: Partial<VerifyResponse> = {}): VerifyResponse {
  return {
    verdict: 'fail',
    strict_citations: false,
    counts: { supported: 1, miscited: 0, uncited_supported: 0, partial: 0, unsupported: 1, no_claim: 1 },
    sentences: [
      sentence(ANSWER, 'The HX-2 carries 60 kg [P1].', 'supported', { cited: ['P1'] }, [{ text: 'HX-2 carries 60 kg', supported: true, evidence: [evidence()] }]),
      sentence(ANSWER, 'It also carries 80 kg [P1].', 'unsupported', { cited: ['P1'] }, [{ text: 'carries 80 kg', supported: false, evidence: [] }]),
      sentence(ANSWER, 'Thanks!', 'no_claim'),
    ],
    passages_unavailable: [],
    judge: { name: 'local', model: 'qwen2.5' },
    usage: { input_tokens: 1830, output_tokens: null, judge_calls: 2 },
    ...over,
  }
}
