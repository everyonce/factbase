# Inference Providers

Factbase uses an embedding model for semantic search and link detection. It supports two backends: Amazon Bedrock (default) and Ollama (self-hosted).

> **Note:** As of Phase 6, factbase no longer uses a server-side LLM. All reasoning (link review, cross-validation, review application) is handled by the client agent via MCP. Only an embedding model is required.

## Amazon Bedrock (default)

Bedrock requires no local model management — models are hosted by AWS. You need:

- AWS credentials (instance profile, SSO, environment variables, or `~/.aws/credentials`)
- The `bedrock` feature enabled at build time
- Model access enabled in the [Bedrock console](https://console.aws.amazon.com/bedrock/home#/modelaccess)

### Build

```bash
cargo build --release --features bedrock
```

### Configuration

Factbase works with zero config if your AWS environment is set up (credentials + us-east-1 region). For explicit configuration:

```yaml
# ~/.config/factbase/config.yaml
embedding:
  provider: bedrock
  model: amazon.nova-2-multimodal-embeddings-v1:0
  dimension: 1024
  region: us-east-1    # AWS region
```

The `region` field specifies the AWS region for Bedrock API calls. The older `base_url` field is still accepted as a deprecated alias.

### Supported models

**Embedding models** (via InvokeModel):
| Model | ID | Dimensions |
|-------|----|------------|
| Titan Embed Text V2 | `amazon.titan-embed-text-v2:0` | 256, 512, 1024 |
| Nova Multimodal Embeddings | `amazon.nova-2-multimodal-embeddings-v1:0` | 256, 384, 1024, 3072 |

### AWS credentials

Factbase uses the standard AWS SDK credential chain. In order of precedence:

1. **Environment variables**: `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY`
2. **Instance profile**: automatic on EC2/ECS/Lambda
3. **SSO/config**: `~/.aws/config` with `aws sso login`
4. **Credentials file**: `~/.aws/credentials`

To verify your credentials work:

```bash
aws sts get-caller-identity
aws bedrock-runtime invoke-model --model-id amazon.titan-embed-text-v2:0 \
  --content-type application/json --accept application/json \
  --body '{"inputText":"test","dimensions":1024,"normalize":true}' /dev/null
```

### Region configuration

The region is set via `region` in config. If omitted, it uses the standard AWS region resolution (environment variable `AWS_REGION`, instance metadata, or `~/.aws/config`).

### Troubleshooting Bedrock

**AccessDeniedException**: Enable model access in the [Bedrock console](https://console.aws.amazon.com/bedrock/home#/modelaccess) for your region.

**ValidationException with cross-region models**: Use the `us.` prefix for cross-region inference (e.g., `us.anthropic.claude-3-5-haiku-20241022-v1:0`).

---

## Ollama (self-hosted)

[Ollama](https://ollama.ai) runs models locally. Useful when you want full control, offline operation, or are experimenting with custom models.

### Setup

```bash
# Install Ollama
curl -fsSL https://ollama.ai/install.sh | sh

# Pull embedding model
ollama pull qwen3-embedding:0.6b    # embeddings (1024 dims, 32K context)
```

### Configuration

```yaml
# ~/.config/factbase/config.yaml
embedding:
  provider: ollama
  base_url: http://localhost:11434
  model: qwen3-embedding:0.6b
  dimension: 1024
  timeout_secs: 30

ollama:
  max_retries: 3
  retry_delay_ms: 1000
```

### Build

No special feature flag needed — Ollama support is always compiled in:

```bash
cargo build --release
```

### Troubleshooting Ollama

**Connection refused**: Start the Ollama server:
```bash
ollama serve &
factbase doctor
```

**Model not found**: Pull the required model:
```bash
ollama pull qwen3-embedding:0.6b
factbase doctor --fix    # auto-pull missing models
```

**Slow generation**: Ollama performance depends on your hardware. GPU acceleration helps significantly. Reduce batch size if running out of memory:
```yaml
processor:
  embedding_batch_size: 5
```

**Timeouts**: Increase the timeout for slow hardware:
```yaml
embedding:
  timeout_secs: 120
```

### Verify setup

```bash
factbase doctor
# ✓ Ollama server: http://localhost:11434 (running)
# ✓ Embedding model: qwen3-embedding:0.6b (available)
```
