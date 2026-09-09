import type { Content, EmbeddingModel, EmbeddingOptions } from "@zvec/zvec-grep";

const ENDPOINT = "http://127.0.0.1:11434/api/embed";
const MODEL = "hf.co/zenmagnets/Nemotron-3-Embed-1B-Q4_K_M-GGUF:latest";
const DIMENSIONS = 2048;
const BATCH_SIZE = 16;
// The quantization publisher tested 4096 tokens. Prefixes follow the NVIDIA model card.
// https://huggingface.co/zenmagnets/Nemotron-3-Embed-1B-Q4_K_M-GGUF
const CONTEXT = 4096;
// zvec budgets characters, not tokenizer output. Keep source fragments below the real context ceiling.
const INDEX_BUDGET = 1024;

function inputsFor(contents: readonly Content[], purpose: EmbeddingOptions["purpose"]): string[] {
  if (contents.length === 0 || contents.length > BATCH_SIZE) {
    throw new Error(`Nemotron requires 1..${BATCH_SIZE} inputs per batch`);
  }
  const prefix = purpose === "query" ? "query: " : "passage: ";
  return contents.map((content) => {
    if (content.kind !== "text" || !content.text.trim()) {
      throw new Error("Nemotron requires nonempty text");
    }
    return prefix + content.text;
  });
}

function validateVector(vector: unknown): number[] {
  if (!Array.isArray(vector) || vector.length !== DIMENSIONS) {
    throw new Error(`Nemotron returned a vector with invalid dimensions; expected ${DIMENSIONS}`);
  }
  let magnitude = 0;
  for (const value of vector) {
    if (typeof value !== "number" || !Number.isFinite(value)) {
      throw new Error("Nemotron returned a non-finite vector");
    }
    magnitude += Math.abs(value);
  }
  if (magnitude === 0) throw new Error("Nemotron returned a zero vector");
  return vector;
}

function validateResponse(payload: unknown, count: number): number[][] {
  const vectors = (payload as { embeddings?: unknown } | null)?.embeddings;
  if (!Array.isArray(vectors) || vectors.length !== count) {
    throw new Error(`Nemotron returned an invalid embedding count; expected ${count}`);
  }
  return vectors.map(validateVector);
}

async function embed(contents: readonly Content[], options: EmbeddingOptions = {}) {
  options.signal?.throwIfAborted();
  const input = inputsFor(contents, options.purpose);
  const deadline = AbortSignal.timeout(120_000);
  const signal = options.signal ? AbortSignal.any([options.signal, deadline]) : deadline;
  try {
    const response = await fetch(ENDPOINT, {
      method: "POST",
      headers: { "content-type": "application/json" },
      redirect: "error",
      signal,
      body: JSON.stringify({
        model: MODEL,
        input,
        // Refuse oversized input; never claim that silently clipped text was indexed.
        truncate: false,
        options: { num_ctx: CONTEXT },
      }),
    });
    if (!response.ok) {
      throw new Error(`Nemotron HTTP ${response.status}: ${(await response.text()).slice(0, 512)}`);
    }
    const vectors = validateResponse(await response.json(), input.length);
    options.onProgress?.({ stage: "ready", model: MODEL });
    return { vectors, truncated: [] };
  } catch (error) {
    signal.throwIfAborted();
    throw error;
  }
}

export function createNemotronModel(): EmbeddingModel {
  return {
    info: {
      reference: `ollama/${MODEL}`,
      provider: "ollama",
      name: MODEL,
      dimension: DIMENSIONS,
      metric: "cosine",
      endpoint: ENDPOINT,
      defaultConcurrency: 1,
      inputKinds: ["text"],
      limits: { maxBatchSize: BATCH_SIZE, maxInputTokens: INDEX_BUDGET },
    },
    embed,
    async dispose() {
      // Ollama is shared. This stateless client owns no model to unload.
    },
  };
}
