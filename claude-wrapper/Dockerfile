# Claude Code OpenAI-compatible wrapper
# Wraps Claude Code CLI (via claude-agent-sdk) as an OpenAI API endpoint
# FlowR MCP is pre-configured as a tool so the agent can manage workflows

FROM python:3.12-slim

RUN apt-get update && apt-get install -y \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Node.js 22 (required by Claude Code CLI)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code CLI globally
RUN npm install -g @anthropic-ai/claude-code

# Poetry
RUN curl -sSL https://install.python-poetry.org | python3 -
ENV PATH="/root/.local/bin:${PATH}"

# Copy wrapper source
COPY wrapper/ /app
WORKDIR /app

# Install Python deps
RUN poetry install --no-root

# Workspace with MCP config — Claude CLI reads .mcp.json from cwd
COPY workspace/ /workspace/

ENV CLAUDE_CWD=/workspace
ENV PORT=8000

EXPOSE 8000

CMD ["poetry", "run", "uvicorn", "src.main:app", "--host", "0.0.0.0", "--port", "8000", "--workers", "1"]
