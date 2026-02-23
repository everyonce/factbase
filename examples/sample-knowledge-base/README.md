# Sample Knowledge Base

Example knowledge base demonstrating factbase capabilities.

## Contents

- `people/` - Team member profiles (4 documents)
- `projects/` - Project documentation (2 documents)
- `concepts/` - Technical concepts (2 documents)

## Usage

```bash
# Add this repository to factbase
factbase repo add sample ./examples/sample-knowledge-base

# Index the documents
factbase scan sample

# Search for information
factbase search "who works on Platform API"

# Start the MCP server
factbase serve
```

## Document Types

Documents are automatically typed based on their parent folder:
- `people/alice-chen.md` → type: "person"
- `projects/platform-api.md` → type: "project"
- `concepts/api-gateway.md` → type: "concept"

## Cross-References

Documents reference each other naturally:
- Alice Chen is tech lead for Platform API
- Carol Davis designed Internal Tools Dashboard
- Platform API implements the API Gateway pattern

Factbase's LLM-powered link detection automatically discovers these relationships.
