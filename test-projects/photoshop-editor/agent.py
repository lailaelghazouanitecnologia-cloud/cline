#!/usr/bin/env python3
import subprocess
import json
import os

API_KEY = "os.environ.get("GROQ_API_KEY", "")"
MODEL = "llama-3.3-70b-versatile"
WORK_DIR = "/home/user/cline/test-projects/photoshop-editor"

TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "shell",
            "description": "Execute a shell command",
            "parameters": {
                "type": "object",
                "properties": {"command": {"type": "string", "description": "Command to execute"}},
                "required": ["command"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write content to a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path"},
                    "content": {"type": "string", "description": "File content"}
                },
                "required": ["path", "content"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read a file",
            "parameters": {
                "type": "object",
                "properties": {"path": {"type": "string", "description": "File path"}},
                "required": ["path"]
            }
        }
    }
]

def call_api(messages):
    payload = {
        "model": MODEL,
        "messages": messages,
        "tools": TOOLS,
        "max_tokens": 4096,
        "temperature": 0.7
    }

    cmd = [
        "curl", "-k", "-s", "-X", "POST",
        "https://api.groq.com/openai/v1/chat/completions",
        "-H", f"Authorization: Bearer {API_KEY}",
        "-H", "Content-Type: application/json",
        "-d", json.dumps(payload)
    ]

    result = subprocess.run(cmd, capture_output=True, text=True)
    return json.loads(result.stdout)

def execute_tool(name, args):
    if name == "shell":
        try:
            result = subprocess.run(
                args["command"], shell=True, capture_output=True, text=True,
                cwd=WORK_DIR, timeout=60
            )
            output = result.stdout + result.stderr
            return output[:2000] if len(output) > 2000 else output
        except Exception as e:
            return f"Error: {e}"

    elif name == "write_file":
        try:
            path = os.path.join(WORK_DIR, args["path"].lstrip("/"))
            os.makedirs(os.path.dirname(path), exist_ok=True) if os.path.dirname(path) else None
            with open(path, "w") as f:
                f.write(args["content"])
            return f"Wrote {len(args['content'])} bytes to {args['path']}"
        except Exception as e:
            return f"Error: {e}"

    elif name == "read_file":
        try:
            path = os.path.join(WORK_DIR, args["path"].lstrip("/"))
            with open(path, "r") as f:
                content = f.read()
            return content[:3000] if len(content) > 3000 else content
        except Exception as e:
            return f"Error: {e}"

    return f"Unknown tool: {name}"

def run_agent(task):
    messages = [
        {"role": "system", "content": f"You are an AI coding assistant. Working directory: {WORK_DIR}. Use tools to complete tasks."},
        {"role": "user", "content": task}
    ]

    for turn in range(1, 31):
        print(f"\n--- Turn {turn} ---")

        response = call_api(messages)

        if "error" in response:
            print(f"API Error: {response['error']}")
            break

        choice = response["choices"][0]
        message = choice["message"]

        if "content" in message and message["content"]:
            print(f"Assistant: {message['content'][:500]}")

        if "tool_calls" not in message or not message["tool_calls"]:
            print("\n--- Task Complete ---")
            break

        messages.append(message)

        for tc in message["tool_calls"]:
            name = tc["function"]["name"]
            args = json.loads(tc["function"]["arguments"])
            print(f"→ {name}: {str(args)[:100]}")

            result = execute_tool(name, args)
            print(f"  ✓ {result[:100]}...")

            messages.append({
                "role": "tool",
                "tool_call_id": tc["id"],
                "content": result
            })

if __name__ == "__main__":
    task = """Create a photoshop-like image editor using Bun and TypeScript:
1. Initialize with: bun init -y
2. Create a canvas-based editor with layers support
3. Add tools: brush, eraser, selection, move
4. Add filters: blur, brightness, contrast
5. Add file operations: open, save as PNG
6. Use dark theme UI
Start now."""

    run_agent(task)
