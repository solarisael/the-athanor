# End-to-end memory evaluation — 2026-07-20

Generated at **2026-07-20T23:54:38.282Z** by [run-end-to-end.ts](./run-end-to-end.ts).

## Contract

This evaluates the complete user-turn → automatic multi-lane recall viewport → model selection → answer path. Reference evidence resolves from current unsuperseded threads at run time, except scenarios explicitly marked historical. Exact memory IDs are not retrieval targets.

- scenarios: **6**
- trials per variant: **3**
- completed trials: **39**
- scenario SHA-256: `b85a2cc53f241b55166dee674f591c35d994937323ec1a6f22d50ea26da614cf`
- answer model request: `active OMP default`
- judge model request: `active OMP default`

## Scenario results

| Scenario | Variant | Passes | Success | Criteria | Evidence available | Risk violations | Unsupported claims |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| a-squawk-standing-verdict | full | 3/3 | 100.0% | 100.0% | 100.0% | 0.00 | 0.00 |
| a-squawk-standing-verdict | no-recall | 0/3 | 0.0% | 33.3% | 0.0% | 0.00 | 0.67 |
| babel-opening-digital-verdict | full | 0/3 | 0.0% | 33.3% | 44.4% | 0.00 | 0.00 |
| babel-opening-digital-verdict | no-recall | 0/3 | 0.0% | 22.2% | 0.0% | 0.33 | 2.00 |
| public-substrate-current-status | full | 0/3 | 0.0% | 50.0% | 50.0% | 0.00 | 2.33 |
| public-substrate-current-status | no-recall | 0/3 | 0.0% | 0.0% | 0.0% | 0.00 | 1.33 |
| public-substrate-current-status | stale-distractor | 0/3 | 0.0% | 50.0% | 50.0% | 0.33 | 2.33 |
| public-substrate-historical-pause | full | 0/3 | 0.0% | 50.0% | 100.0% | 0.00 | 0.00 |
| public-substrate-historical-pause | no-recall | 0/3 | 0.0% | 0.0% | 0.0% | 0.00 | 0.00 |
| sol-work-language-production-default | full | 3/3 | 100.0% | 100.0% | 100.0% | 0.00 | 0.00 |
| sol-work-language-production-default | no-recall | 0/3 | 0.0% | 33.3% | 0.0% | 0.00 | 2.00 |
| tuner-current-speech-standard | full | 0/3 | 0.0% | 0.0% | 0.0% | 1.00 | 4.67 |
| tuner-current-speech-standard | no-recall | 0/3 | 0.0% | 0.0% | 0.0% | 1.00 | 5.33 |

## Memory lift

| Scenario | Full | No recall | Lift |
| --- | ---: | ---: | ---: |
| tuner-current-speech-standard | 0.0% | 0.0% | +0.0 pp |
| public-substrate-current-status | 0.0% | 0.0% | +0.0 pp |
| public-substrate-historical-pause | 0.0% | 0.0% | +0.0 pp |
| babel-opening-digital-verdict | 0.0% | 0.0% | +0.0 pp |
| a-squawk-standing-verdict | 100.0% | 0.0% | +100.0 pp |
| sol-work-language-production-default | 100.0% | 0.0% | +100.0 pp |

## Retrieval viewport

| Scenario | Raw candidates | Kept | Suppressed | Reference records |
| --- | ---: | ---: | ---: | ---: |
| tuner-current-speech-standard | 8 | 5 | 0 | 2 |
| public-substrate-current-status | 12 | 5 | 0 | 2 |
| public-substrate-historical-pause | 10 | 4 | 1 | 1 |
| babel-opening-digital-verdict | 8 | 5 | 0 | 1 |
| a-squawk-standing-verdict | 7 | 4 | 1 | 1 |
| sol-work-language-production-default | 11 | 4 | 1 | 1 |

## Trial detail

| Scenario | Variant | Trial | Pass | Failure class | Answer model | Judge model |
| --- | --- | ---: | --- | --- | --- | --- |
| tuner-current-speech-standard | full | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| tuner-current-speech-standard | full | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| tuner-current-speech-standard | full | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| tuner-current-speech-standard | no-recall | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| tuner-current-speech-standard | no-recall | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| tuner-current-speech-standard | no-recall | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | full | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | full | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | full | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | no-recall | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | no-recall | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | no-recall | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | stale-distractor | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | stale-distractor | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-current-status | stale-distractor | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-historical-pause | full | 1 | no | selection-or-synthesis | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-historical-pause | full | 2 | no | selection-or-synthesis | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-historical-pause | full | 3 | no | selection-or-synthesis | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-historical-pause | no-recall | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-historical-pause | no-recall | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| public-substrate-historical-pause | no-recall | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| babel-opening-digital-verdict | full | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| babel-opening-digital-verdict | full | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| babel-opening-digital-verdict | full | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| babel-opening-digital-verdict | no-recall | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| babel-opening-digital-verdict | no-recall | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| babel-opening-digital-verdict | no-recall | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| a-squawk-standing-verdict | full | 1 | yes | pass | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| a-squawk-standing-verdict | full | 2 | yes | pass | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| a-squawk-standing-verdict | full | 3 | yes | pass | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| a-squawk-standing-verdict | no-recall | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| a-squawk-standing-verdict | no-recall | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| a-squawk-standing-verdict | no-recall | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| sol-work-language-production-default | full | 1 | yes | pass | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| sol-work-language-production-default | full | 2 | yes | pass | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| sol-work-language-production-default | full | 3 | yes | pass | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| sol-work-language-production-default | no-recall | 1 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| sol-work-language-production-default | no-recall | 2 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |
| sol-work-language-production-default | no-recall | 3 | no | retrieval | openai-codex/gpt-5.6-sol | openai-codex/gpt-5.6-sol |

The JSON report contains the exact viewports, dynamically resolved reference records, answers, citations, judge decisions, usage, and latency for audit.
