"""MCP Server — expose LLMBase as a Model Context Protocol server.

Tools are generated from ``tools.operations`` so this surface never drifts
from the CLI / HTTP definitions. Register a custom operation via
``tools.operations.register`` and it appears here automatically.

Usage:
    python -m tools.mcp_server [--base-dir .]

Or register in a Claude Code / Cursor / Claude Desktop config::

    {
      "mcpServers": {
        "llmbase": {
          "command": "python",
          "args": ["-m", "tools.mcp_server", "--base-dir", "/path/to/kb"]
        }
      }
    }
"""

import argparse
import asyncio
import json
import logging
import sys
from pathlib import Path

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import Tool, TextContent

from . import operations as ops

logging.basicConfig(level=logging.INFO, stream=sys.stderr)
logger = logging.getLogger("llmbase.mcp")


def _tools() -> list[Tool]:
    return [
        Tool(name=op.name, description=op.description, inputSchema=op.params)
        for op in ops.all_operations()
    ]


def handle_tool(name: str, arguments: dict, base_dir: Path) -> str:
    """Back-compat shim: synchronously dispatch a tool and return text.

    Kept for callers that pre-date the operations-contract refactor
    (tests, older integration scripts). New code should import
    ``tools.operations.dispatch`` directly.
    """
    if ops.get(name) is None:
        return f"Unknown tool: {name}"
    try:
        result = ops.dispatch(name, base_dir, arguments or {})
    except RuntimeError as e:
        # Match legacy message for lock contention
        return f"Another write operation is running. {e}"
    return _format(result)


def _format(result) -> str:
    """Render an operation's return value for MCP text output."""
    if isinstance(result, str):
        return result
    try:
        return json.dumps(result, ensure_ascii=False, indent=2, default=str)
    except (TypeError, ValueError):
        return str(result)


def create_server(base_dir: Path) -> Server:
    server = Server("llmbase")

    @server.list_tools()
    async def list_tools():
        return _tools()

    @server.call_tool()
    async def call_tool(name: str, arguments: dict):
        if ops.get(name) is None:
            raise ValueError(f"Unknown tool: {name}")
        try:
            result = await asyncio.to_thread(ops.dispatch, name, base_dir, arguments or {})
            return [TextContent(type="text", text=_format(result))]
        except RuntimeError as e:
            # Lock contention — render as normal text instead of surfacing as error
            return [TextContent(type="text", text=f"Busy: {e}")]
        except Exception as e:
            logger.error(f"Tool {name} failed: {e}")
            raise

    return server


async def main():
    parser = argparse.ArgumentParser(description="LLMBase MCP Server")
    parser.add_argument("--base-dir", type=str, default=".", help="Knowledge base directory")
    args = parser.parse_args()

    base_dir = Path(args.base_dir).resolve()
    logger.info(f"Starting LLMBase MCP server (base: {base_dir})")

    server = create_server(base_dir)
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
