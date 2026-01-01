# Cline Agent

AI coding agent built in Rust.

## Structure

```
├── agent-core/     # Core agent framework (Rust)
└── legacy/         # Original Cline VS Code extension
```

## agent-core

Modular Rust workspace:

- `common` - Error types, identifiers, limits
- `protocol` - Event/submission system, session state machine
- `config` - Configuration, features, providers
- `tools` - Tool abstraction and handlers
- `exec` - Turn management, tool execution
- `core` - Agent interface, session management

### Build

```bash
cd agent-core
cargo build
```

### Architecture

```
┌─────────────────────────────────────────┐
│                Agent                     │
│  submit(Operation) → SubmissionId       │
│  next_event() → Event                   │
└───────────────┬─────────────────────────┘
                │
┌───────────────▼─────────────────────────┐
│              Session                     │
│  ┌─────────────┐  ┌──────────────────┐  │
│  │StateManager │  │    Executor      │  │
│  └─────────────┘  └────────┬─────────┘  │
└────────────────────────────┼────────────┘
                             │
┌────────────────────────────▼────────────┐
│            ToolRouter                    │
│  ┌─────────────────────────────────┐    │
│  │        ToolRegistry             │    │
│  └─────────────────────────────────┘    │
└─────────────────────────────────────────┘
```

## Legacy

The original Cline VS Code extension is preserved in `legacy/`.
