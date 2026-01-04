#!/usr/bin/env python3
import os
import json
import requests
import urllib3
urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)

API_KEY = "os.environ.get("GROQ_API_KEY", "")"
BASE_URL = "https://api.groq.com/openai/v1"

tools = [
    {
        "type": "function",
        "function": {
            "name": "shell",
            "description": "Execute a shell command",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The command to execute"}
                },
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
    }
]

def chat(messages):
    response = requests.post(
        f"{BASE_URL}/chat/completions",
        headers={
            "Authorization": f"Bearer {API_KEY}",
            "Content-Type": "application/json"
        },
        json={
            "model": "llama-3.3-70b-versatile",
            "messages": messages,
            "tools": tools,
            "max_tokens": 4096,
            "temperature": 0.7
        },
        timeout=60.0,
        verify=False
    )
    print(f"Status: {response.status_code}")
    print(f"Response: {response.text[:500]}")
    return response.json() if response.ok else {"error": response.text}

if __name__ == "__main__":
    messages = [
        {"role": "system", "content": "You are an AI coding assistant. Use the tools to complete tasks."},
        {"role": "user", "content": "Create a simple hello world file called hello.txt"}
    ]

    result = chat(messages)
    print(json.dumps(result, indent=2))
