"""Allow running MCP server as: python -m tools.mcp_server"""
from .mcp_server import main
import asyncio

asyncio.run(main())
